use adw::{prelude::*, subclass::prelude::*};
use gtk::{gio, glib};

use crate::{
    about,
    config::{APP_ID, PKGDATADIR, PROFILE, VERSION},
    settings::Settings,
    window::Window,
};

mod imp {
    use super::*;

    #[derive(Debug, Default)]
    pub struct Application {
        pub(super) settings: Settings,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Application {
        const NAME: &'static str = "DaggerApplication";
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

            let window = Window::new(&obj);
            window.present();
        }

        fn startup(&self) {
            self.parent_startup();

            let obj = self.obj();

            gtk::Window::set_default_icon_name(APP_ID);

            obj.setup_gactions();
            obj.setup_accels();
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
            .property("resource-base-path", "/io/github/seadve/Dagger/")
            .build()
    }

    pub fn settings(&self) -> &Settings {
        &self.imp().settings
    }

    fn setup_gactions(&self) {
        let action_quit = gio::ActionEntry::builder("quit")
            .activate(move |obj: &Self, _, _| {
                todo!();
            })
            .build();
        let action_about = gio::ActionEntry::builder("about")
            .activate(|obj: &Self, _, _| {
                about::present_window(obj.active_window().as_ref());
            })
            .build();
        self.add_action_entries([action_quit, action_about]);
    }

    fn setup_accels(&self) {
        self.set_accels_for_action("app.quit", &["<Control>q"]);
        self.set_accels_for_action("win.new-document", &["<Control>n"]);
        self.set_accels_for_action("win.open-document", &["<Control>o"]);
        self.set_accels_for_action("win.save-document", &["<Control>s"]);
        self.set_accels_for_action("win.save-document-as", &["<Shift><Control>s"]);
    }

    pub fn run(&self) -> glib::ExitCode {
        tracing::info!("Dagger ({})", APP_ID);
        tracing::info!("Version: {} ({})", VERSION, PROFILE);
        tracing::info!("Datadir: {}", PKGDATADIR);

        ApplicationExtManual::run(self)
    }
}

impl Default for Application {
    /// Returns the static instance of `Application`.
    ///
    /// # Panics
    /// Panics if the app is not running or if this is called on a non-main thread.
    fn default() -> Self {
        debug_assert!(
            gtk::is_initialized_main_thread(),
            "application must only be accessed in the main thread"
        );

        gio::Application::default().unwrap().downcast().unwrap()
    }
}
