use gettextrs::gettext;
use gtk::{
    glib::{self, IsA},
    prelude::*,
};

use std::{env, path::Path};

use crate::{
    config::{APP_ID, VERSION},
    utils,
};

pub fn present_window(transient_for: Option<&impl IsA<gtk::Window>>) {
    let win = adw::AboutWindow::builder()
        .modal(true)
        .application_icon(APP_ID)
        .application_name(utils::application_name())
        .developer_name(gettext("Dave Patrick Caberto"))
        .version(VERSION)
        .copyright(gettext("© 2023 Dave Patrick Caberto"))
        .license_type(gtk::License::Gpl30)
        // Translators: Replace "translator-credits" with your names. Put a comma between.
        .translator_credits(gettext("translator-credits"))
        .issue_url("https://github.com/SeaDve/Dagger/issues")
        .support_url("https://github.com/SeaDve/Dagger/discussions")
        .debug_info(debug_info())
        .debug_info_filename("dagger-debug-info")
        .build();

    win.add_link(&gettext("Donate"), "https://seadve.github.io/donate/");
    win.add_link(
        &gettext("Donate (Buy Me a Coffee)"),
        "https://www.buymeacoffee.com/seadve",
    );
    win.add_link(&gettext("GitHub"), "https://github.com/SeaDve/Dagger");
    win.add_link(
        &gettext("Translate"),
        "https://hosted.weblate.org/projects/kooha/dagger",
    );

    win.set_transient_for(transient_for);
    win.present();
}

fn debug_info() -> String {
    let is_flatpak = Path::new("/.flatpak-info").exists();

    let language_names = glib::language_names().join(", ");

    let distribution = glib::os_info("PRETTY_NAME").unwrap_or_else(|| "<unknown>".into());
    let desktop_session = env::var("DESKTOP_SESSION").unwrap_or_else(|_| "<unknown>".into());
    let display_server = env::var("XDG_SESSION_TYPE").unwrap_or_else(|_| "<unknown>".into());

    let gtk_version = format!(
        "{}.{}.{}",
        gtk::major_version(),
        gtk::minor_version(),
        gtk::micro_version()
    );
    let adw_version = format!(
        "{}.{}.{}",
        adw::major_version(),
        adw::minor_version(),
        adw::micro_version()
    );
    let webkit_version = format!(
        "{}.{}.{}",
        webkit::functions::major_version(),
        webkit::functions::minor_version(),
        webkit::functions::micro_version()
    );

    format!(
        r#"- {APP_ID} {VERSION}
- Flatpak: {is_flatpak}

- Language: {language_names}

- Distribution: {distribution}
- Desktop Session: {desktop_session}
- Display Server: {display_server}

- GTK {gtk_version}
- Libadwaita {adw_version}
- Webkit {webkit_version}"#
    )
}
