use gsettings_macro::gen_settings;
use gtk::{gio, glib};

use crate::{config::APP_ID, graph_view::Engine};

#[gen_settings(file = "./data/io.github.seadve.Dagger.gschema.xml.in")]
#[gen_settings_define(key_name = "layout-engine", arg_type = "Engine", ret_type = "Engine")]
pub struct Settings;

impl Default for Settings {
    fn default() -> Self {
        Self::new(APP_ID)
    }
}
