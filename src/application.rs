use adw::{prelude::*, subclass::prelude::*};
use gtk::{
    gio,
    glib::{self, clone},
};

use crate::{
    about,
    config::{APP_ID, PKGDATADIR, PROFILE, VERSION},
    settings::Settings,
    utils,
    window::Window,
};

mod imp {
    use super::*;
    use glib::WeakRef;
    use std::cell::OnceCell;

    #[derive(Debug, Default)]
    pub struct Application {
        pub(super) window: OnceCell<WeakRef<Window>>,
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

            if let Some(window) = self.window.get() {
                let window = window.upgrade().unwrap();
                window.present();
                return;
            }

            let window = Window::new(&obj);
            self.window
                .set(window.downgrade())
                .expect("Window already set.");

            obj.window().present();
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
    pub fn settings(&self) -> &Settings {
        &self.imp().settings
    }

    fn window(&self) -> Window {
        self.imp().window.get().unwrap().upgrade().unwrap()
    }

    fn setup_gactions(&self) {
        let action_quit = gio::ActionEntry::builder("quit")
            .activate(move |obj: &Self, _, _| {
                obj.window().close();
            })
            .build();
        let action_about = gio::ActionEntry::builder("about")
            .activate(|obj: &Self, _, _| {
                utils::spawn(
                    glib::Priority::default(),
                    clone!(@weak obj => async move {
                        about::present_window(Some(&obj.window())).await;
                    }),
                );
            })
            .build();
        self.add_action_entries([action_quit, action_about]);
    }

    fn setup_accels(&self) {
        self.set_accels_for_action("app.quit", &["<Control>q"]);
        self.set_accels_for_action("window.close", &["<Control>w"]);
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
    fn default() -> Self {
        glib::Object::builder()
            .property("application-id", APP_ID)
            .property("resource-base-path", "/io/github/seadve/Dagger/")
            .build()
    }
}
