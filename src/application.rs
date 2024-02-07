use adw::{prelude::*, subclass::prelude::*};
use gtk::{
    gio,
    glib::{self, clone},
};

use crate::{
    about,
    config::{APP_ID, PKGDATADIR, PROFILE, VERSION},
    save_changes_dialog,
    session::Session,
    utils,
    window::Window,
};

mod imp {
    use super::*;

    #[derive(Debug, Default)]
    pub struct Application {
        pub(super) session: Session,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Application {
        const NAME: &'static str = "DelineateApplication";
        type Type = super::Application;
        type ParentType = adw::Application;
    }

    impl ObjectImpl for Application {}

    impl ApplicationImpl for Application {
        fn activate(&self) {
            self.parent_activate();

            let obj = self.obj();

            if let Some(window) = obj.windows().first() {
                window.present();
                return;
            }

            let hold_guard = obj.hold();
            utils::spawn(clone!(@weak obj => async move {
                tracing::debug!("Restoring session on activate");

                let _hold_guard = hold_guard;

                let session = obj.session();
                if let Err(err) = session.restore().await {
                    tracing::error!("Failed to restore session: {:?}", err);

                    let window = session.add_new_window();
                    window.present();
                }
            }));
        }

        fn startup(&self) {
            self.parent_startup();

            let obj = self.obj();

            gtk::Window::set_default_icon_name(APP_ID);

            obj.setup_gactions();
            obj.setup_accels();
        }

        fn open(&self, files: &[gio::File], _hint: &str) {
            let obj = self.obj();

            let window = if let Some(active_window) = obj.active_window() {
                active_window.downcast::<Window>().unwrap()
            } else if let Some(window) = obj.windows().first() {
                window.clone().downcast::<Window>().unwrap()
            } else {
                self.session.add_new_window()
            };
            self.session.open_files(files, &window);
        }
    }

    impl GtkApplicationImpl for Application {}
    impl AdwApplicationImpl for Application {}
}

glib::wrapper! {
    pub struct Application(ObjectSubclass<imp::Application>)
        @extends gio::Application, gtk::Application,
        @implements gio::ActionMap, gio::ActionGroup;
}

impl Application {
    pub fn new() -> Self {
        glib::Object::builder()
            .property("application-id", APP_ID)
            .property("resource-base-path", "/io/github/seadve/Delineate/")
            .property("flags", gio::ApplicationFlags::HANDLES_OPEN)
            .build()
    }

    /// Returns the static instance of `Application`.
    ///
    /// # Panics
    /// Panics if the app is not running or if this is called on a non-main thread.
    pub fn get() -> Self {
        debug_assert!(
            gtk::is_initialized_main_thread(),
            "application must only be accessed in the main thread"
        );

        gio::Application::default().unwrap().downcast().unwrap()
    }

    pub fn session(&self) -> &Session {
        &self.imp().session
    }

    pub fn run(&self) -> glib::ExitCode {
        tracing::info!("Delineate ({})", APP_ID);
        tracing::info!("Version: {} ({})", VERSION, PROFILE);
        tracing::info!("Datadir: {}", PKGDATADIR);

        ApplicationExtManual::run(self)
    }

    pub fn quit(&self) {
        utils::spawn(clone!(@weak self as obj => async move {
            if obj.quit_request().await.is_proceed() {
                tracing::debug!("Saving session on quit");

                if let Err(err) = obj.session().save().await {
                    tracing::error!("Failed to save session on quit: {:?}", err);
                }

                ApplicationExt::quit(&obj);
            }
        }));
    }

    /// Returns `Proceed` if quit process shall proceed, `Stop` if it shall be aborted.
    async fn quit_request(&self) -> glib::Propagation {
        let unsaved_documents = self
            .session()
            .windows()
            .iter()
            .flat_map(|windows| windows.pages())
            .map(|page| page.document())
            .filter(|document| document.is_modified())
            .collect::<Vec<_>>();

        if unsaved_documents.is_empty() {
            return glib::Propagation::Proceed;
        }

        let active_window = self.active_window().unwrap().downcast::<Window>().unwrap();
        save_changes_dialog::run(&active_window, &unsaved_documents).await
    }

    fn setup_gactions(&self) {
        let action_new_window = gio::ActionEntry::builder("new-window")
            .activate(|obj: &Self, _, _| {
                let window = obj.session().add_new_window();
                window.present();
            })
            .build();
        let action_quit = gio::ActionEntry::builder("quit")
            .activate(move |obj: &Self, _, _| obj.quit())
            .build();
        let action_about = gio::ActionEntry::builder("about")
            .activate(|obj: &Self, _, _| {
                if let Some(window) = obj.active_window() {
                    about::present_dialog(&window);
                } else {
                    tracing::warn!("Can't present about dialog without an active window");
                }
            })
            .build();
        self.add_action_entries([action_new_window, action_quit, action_about]);
    }

    fn setup_accels(&self) {
        self.set_accels_for_action("app.new-window", &["<Control>n"]);
        self.set_accels_for_action("app.quit", &["<Control>q"]);
    }
}
