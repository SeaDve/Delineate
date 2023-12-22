use std::future::Future;

use gettextrs::gettext;
use gtk::{gio, glib};

use crate::config::PROFILE;

pub fn is_devel_profile() -> bool {
    PROFILE == "Devel"
}

/// Spawns a future in the default [`glib::MainContext`]
pub fn spawn<R, F>(priority: glib::Priority, fut: F) -> glib::JoinHandle<R>
where
    R: 'static,
    F: Future<Output = R> + 'static,
{
    let ctx = glib::MainContext::default();
    ctx.spawn_local_with_priority(priority, fut)
}

pub fn graphviz_file_filters() -> gio::ListStore {
    let filter = gtk::FileFilter::new();
    // Translators: DOT is an acronym, do not translate.
    filter.set_name(Some(&gettext("Graphviz DOT Files")));
    filter.add_mime_type("text/vnd.graphviz");

    let filters = gio::ListStore::new::<gtk::FileFilter>();
    filters.append(&filter);
    filters
}
