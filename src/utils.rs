use std::{future::Future, path::Path};

use gettextrs::gettext;
use gtk::{gio, glib, prelude::*};

use crate::config::PROFILE;

pub fn application_name() -> String {
    gettext("Delineate")
}

pub fn is_devel_profile() -> bool {
    PROFILE == "Devel"
}

/// Spawns a future in the default [`glib::MainContext`] with the given priority.
pub fn spawn_with_priority<R, F>(priority: glib::Priority, fut: F) -> glib::JoinHandle<R>
where
    R: 'static,
    F: Future<Output = R> + 'static,
{
    let ctx = glib::MainContext::default();
    ctx.spawn_local_with_priority(priority, fut)
}

/// Spawns a future in the default [`glib::MainContext`].
pub fn spawn<R, F>(fut: F) -> glib::JoinHandle<R>
where
    R: 'static,
    F: Future<Output = R> + 'static,
{
    spawn_with_priority(glib::Priority::default(), fut)
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

pub fn display_file_stem(file: &gio::File) -> String {
    file.path()
        .unwrap()
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string()
}

pub fn display_file_parent(file: &gio::File) -> String {
    if let Some(parent) = file.parent() {
        display_file(&parent)
    } else {
        "/".to_string()
    }
}

pub fn display_file(file: &gio::File) -> String {
    if file.is_native() {
        display_path(&file.path().unwrap())
    } else {
        file.uri().to_string()
    }
}

fn display_path(path: &Path) -> String {
    let home_dir = glib::home_dir();

    if path == home_dir {
        return "~/".to_string();
    }

    let path_display = path.display().to_string();

    if path.starts_with(&home_dir) {
        let home_dir_display = home_dir.display().to_string();
        return format!("~{}", &path_display[home_dir_display.len()..]);
    }

    path_display
}
