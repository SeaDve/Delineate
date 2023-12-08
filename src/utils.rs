use std::future::Future;

use gtk::{gio, glib, prelude::*};

use crate::application::Application;

/// Spawns a future in the default [`glib::MainContext`]
pub fn spawn<R, F>(priority: glib::Priority, fut: F) -> glib::JoinHandle<R>
where
    R: 'static,
    F: Future<Output = R> + 'static,
{
    let ctx = glib::MainContext::default();
    ctx.spawn_local_with_priority(priority, fut)
}

/// Get the global instance of `Application`.
///
/// # Panics
/// Panics if the application is not running or if this is
/// called on a non-main thread.
pub fn app_instance() -> Application {
    debug_assert!(
        gtk::is_initialized_main_thread(),
        "application must only be accessed in the main thread"
    );

    gio::Application::default().unwrap().downcast().unwrap()
}
