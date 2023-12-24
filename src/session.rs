use std::{fs, path::PathBuf, time::Instant};

use anyhow::Result;
use gtk::{
    gio,
    glib::{self, clone, once_cell::sync::Lazy},
    prelude::*,
    subclass::prelude::*,
};
use serde::{Deserialize, Serialize};

use crate::{config::APP_ID, graph_view::LayoutEngine, window::Window, Application};

const DEFAULT_WINDOW_WIDTH: i32 = 1000;
const DEFAULT_WINDOW_HEIGHT: i32 = 600;

static APP_DATA_DIR: Lazy<PathBuf> = Lazy::new(|| {
    let mut path = glib::user_data_dir();
    path.push(APP_ID);
    path
});

#[derive(Debug, Serialize, Deserialize)]
struct Selection {
    start_line: i32,
    start_line_offset: i32,
    end_line: i32,
    end_line_offset: i32,
}

#[derive(Debug, Serialize, Deserialize)]
struct PageState {
    paned_position: i32,
    is_active: bool,
    uri: String,
    selection: Option<Selection>,
    layout_engine: LayoutEngine,
}

#[derive(Debug, Serialize, Deserialize)]
struct WindowState {
    width: i32,
    height: i32,
    is_maximized: bool,
    is_active: bool,
    pages: Vec<PageState>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct State {
    window_width: i32,
    window_height: i32,
    windows: Vec<WindowState>,
}

mod imp {
    use std::cell::{Cell, RefCell};

    use super::*;

    pub struct Session {
        pub(super) state_file: gio::File,
        pub(super) windows: RefCell<Vec<Window>>,
        pub(super) default_window_width: Cell<i32>,
        pub(super) default_window_height: Cell<i32>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Session {
        const NAME: &'static str = "DaggerSession";
        type Type = super::Session;

        fn new() -> Self {
            Self {
                state_file: gio::File::for_path(APP_DATA_DIR.join("state.bin")),
                windows: RefCell::new(Vec::new()),
                default_window_width: Cell::new(DEFAULT_WINDOW_WIDTH),
                default_window_height: Cell::new(DEFAULT_WINDOW_HEIGHT),
            }
        }
    }

    impl ObjectImpl for Session {
        fn dispose(&self) {
            let obj = self.obj();

            // FIXME This should be spawned asynchronously and called in last window closed
            let ctx = glib::MainContext::default();
            ctx.block_on(clone!(@weak obj => async move {
                tracing::debug!("Saving session on dispose");

                if let Err(err) = obj.save().await {
                    tracing::error!("Failed to save session on dispose: {:?}", err);
                }
            }));
        }
    }
}

glib::wrapper! {
    pub struct Session(ObjectSubclass<imp::Session>);
}

impl Session {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn instance() -> Self {
        Application::instance().session().clone()
    }

    pub fn windows(&self) -> Vec<Window> {
        self.imp().windows.borrow().clone()
    }

    pub fn add_new_raw_window(&self) -> Window {
        let imp = self.imp();

        let app = Application::instance();
        let window = Window::new(&app);

        let group = gtk::WindowGroup::new();
        group.add_window(&window);

        imp.windows.borrow_mut().push(window.clone());

        window
    }

    pub fn add_new_window(&self) -> Window {
        let imp = self.imp();

        let window = self.add_new_raw_window();

        let raw_default_width = imp.default_window_width.get();
        let raw_default_height = imp.default_window_height.get();
        let (default_width, default_height) = if raw_default_width > 0 && raw_default_height > 0 {
            (raw_default_width, raw_default_height)
        } else {
            (DEFAULT_WINDOW_WIDTH, DEFAULT_WINDOW_HEIGHT)
        };
        window.set_default_size(default_width, default_height);

        let page = window.add_new_page();
        page.set_paned_position(default_width / 2);

        window
    }

    pub fn remove_window(&self, window: &Window) {
        let imp = self.imp();

        if matches!(imp.windows.borrow().as_slice(), [last_window] if last_window == window) {
            imp.default_window_width.set(window.default_width());
            imp.default_window_height.set(window.default_height());
        }

        imp.windows.borrow_mut().retain(|w| w != window);
    }

    pub async fn restore(&self) -> Result<()> {
        let imp = self.imp();

        let now = Instant::now();

        let state = match imp.state_file.load_bytes_future().await {
            Ok((bytes, _)) => bincode::deserialize::<State>(&bytes)?,
            Err(err) => {
                if !err.matches(gio::IOErrorEnum::NotFound) {
                    return Err(err.into());
                }

                State::default()
            }
        };

        imp.default_window_width.set(state.window_width);
        imp.default_window_height.set(state.window_height);

        let mut active_window = None;
        for window_state in &state.windows {
            let window = self.add_new_raw_window();
            window.set_default_size(window_state.width, window_state.height);
            window.set_maximized(window_state.is_maximized);

            let mut active_page = None;
            for page_state in &window_state.pages {
                let page = window.add_new_page();
                page.set_paned_position(page_state.paned_position);
                page.set_layout_engine(page_state.layout_engine);

                let file = gio::File::for_uri(&page_state.uri);
                if let Err(err) = page.load_file(file).await {
                    tracing::error!(
                        uri = page_state.uri,
                        "Failed to load file for page: {:?}",
                        err
                    );
                }

                if let Some(Selection {
                    start_line,
                    start_line_offset,
                    end_line,
                    end_line_offset,
                }) = page_state.selection
                {
                    let document = page.document();
                    let start_iter = document
                        .iter_at_line_offset(start_line, start_line_offset)
                        .unwrap();
                    let end_iter = document
                        .iter_at_line_offset(end_line, end_line_offset)
                        .unwrap();
                    document.select_range(&start_iter, &end_iter);
                }

                if page_state.is_active {
                    let prev_value = active_page.replace(page);
                    debug_assert!(prev_value.is_none());
                }
            }

            if let Some(page) = active_page {
                window.set_selected_page(&page);
            }

            window.present();

            if window_state.is_active {
                let prev_value = active_window.replace(window);
                debug_assert!(prev_value.is_none());
            }
        }

        if let Some(window) = active_window {
            window.present();
        }

        if state.windows.is_empty() {
            let window = self.add_new_window();
            window.present();
        }

        tracing::debug!(elapsed = ?now.elapsed(), ?state, "Session restored");

        Ok(())
    }

    pub async fn save(&self) -> Result<()> {
        let imp = self.imp();

        let now = Instant::now();

        let mut windows_state = Vec::new();
        for window in imp.windows.borrow().as_slice() {
            let window = window.downcast_ref::<Window>().unwrap();

            let pages = window.pages();

            let mut pages_state = Vec::new();
            for page in pages {
                let document = page.document();

                if document.is_safely_discardable() {
                    continue;
                }

                let Some(file) = &document.file() else {
                    continue;
                };

                let selection = document.selection_bounds().map(|(start, end)| Selection {
                    start_line: start.line(),
                    start_line_offset: start.line_offset(),
                    end_line: end.line(),
                    end_line_offset: end.line_offset(),
                });
                pages_state.push(PageState {
                    paned_position: page.paned_position(),
                    is_active: window.selected_page().as_ref() == Some(&page),
                    uri: file.uri().into(),
                    selection,
                    layout_engine: page.layout_engine(),
                });
            }

            if pages_state.is_empty() {
                continue;
            }

            windows_state.push(WindowState {
                width: window.default_width(),
                height: window.default_height(),
                is_maximized: window.is_maximized(),
                is_active: window.is_active(),
                pages: pages_state,
            });
        }

        let state = State {
            windows: windows_state,
            window_width: imp.default_window_width.get(),
            window_height: imp.default_window_height.get(),
        };
        let bytes = bincode::serialize(&state)?;

        fs::create_dir_all(APP_DATA_DIR.as_path())?;

        imp.state_file
            .replace_contents_future(
                bytes,
                None,
                false,
                gio::FileCreateFlags::REPLACE_DESTINATION,
            )
            .await
            .map_err(|(_, err)| err)?;

        tracing::debug!(elapsed = ?now.elapsed(), ?state, "Session saved");

        Ok(())
    }
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}
