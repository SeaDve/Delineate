#![warn(
    rust_2018_idioms,
    clippy::items_after_statements,
    clippy::needless_pass_by_value,
    clippy::semicolon_if_nothing_returned,
    clippy::match_wildcard_for_single_variants,
    clippy::inefficient_to_string,
    clippy::map_unwrap_or,
    clippy::implicit_clone,
    clippy::struct_excessive_bools,
    clippy::unreadable_literal,
    clippy::if_not_else,
    clippy::doc_markdown,
    clippy::unused_async,
    clippy::default_trait_access,
    clippy::unnecessary_wraps,
    clippy::unused_self,
    clippy::dbg_macro,
    clippy::todo,
    clippy::map_unwrap_or,
    clippy::or_fun_call,
    clippy::print_stdout
)]

mod about;
mod application;
mod config;
mod document;
mod drag_overlay;
mod error_gutter_renderer;
mod graph_view;
mod i18n;
mod settings;
mod utils;
mod window;

use std::ffi::c_char;

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

    test();

    let app = Application::default();
    app.run()
}

fn test() {
    use std::ffi::CString;

    use graphviz_sys::*;

    unsafe {
        // agseterr(AGERR);
        // agseterr(vizErrorf);

        let dot_source = CString::new("digraph { a -> b }").unwrap();
        let graph = agmemread(dot_source.as_ptr());

        let gvc = gvContext();
        let input_format = CString::new("dot").unwrap();
        let output_format = CString::new("svg").unwrap();

        let layout_error = gvLayout(gvc, graph, input_format.as_ptr());
        dbg!(layout_error);

        let mut data = std::ptr::null_mut();
        let mut data_size = 0;
        let ret = gvRenderData(gvc, graph, output_format.as_ptr(), data, &mut data_size);
        dbg!(ret);

        gvFreeLayout(gvc, graph);
        agclose(graph);
        gvFreeContext(gvc);

        let s = String::from_raw_parts(*data as _, data_size as _, data_size as _);
        dbg!(s);
    }
}
