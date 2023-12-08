mod about;
mod application;
mod config;
mod graphviz;
mod settings;
mod utils;
mod window;

use gettextrs::{gettext, LocaleCategory};
use gtk::{gio, glib};

use self::{
    application::Application,
    config::{GETTEXT_PACKAGE, LOCALEDIR, RESOURCES_FILE},
};

fn main() -> glib::ExitCode {
    tracing_subscriber::fmt::init();

    gtk::init().unwrap();
    gtk_source::init();

    gettextrs::setlocale(LocaleCategory::LcAll, "");
    gettextrs::bindtextdomain(GETTEXT_PACKAGE, LOCALEDIR).expect("Unable to bind the text domain");
    gettextrs::textdomain(GETTEXT_PACKAGE).expect("Unable to switch to the text domain");

    glib::set_application_name(&gettext("Dagger"));

    let res = gio::Resource::load(RESOURCES_FILE).expect("Could not load gresource file");
    gio::resources_register(&res);

    let app = Application::default();
    app.run()
}
