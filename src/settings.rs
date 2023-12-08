use gsettings_macro::gen_settings;
use gtk::{gio, glib};

use crate::config::APP_ID;

#[gen_settings(file = "./data/io.github.seadve.Dagger.gschema.xml.in")]
pub struct Settings;

impl Default for Settings {
    fn default() -> Self {
        Self::new(APP_ID)
    }
}
