use std::{fs, path::PathBuf, time::Instant};

use anyhow::Result;
use gettextrs::gettext;
use gtk::{
    gio,
    glib::{self, clone, once_cell::sync::Lazy},
    prelude::*,
    subclass::prelude::*,
};
use serde::{Deserialize, Serialize};

use crate::{
    config::APP_ID, document::Document, graph_view::LayoutEngine, page::Page, utils,
    window::Window, Application,
};

const DEFAULT_WINDOW_WIDTH: i32 = 1000;
const DEFAULT_WINDOW_HEIGHT: i32 = 600;

static APP_DATA_DIR: Lazy<PathBuf> = Lazy::new(|| {
    let mut path = glib::user_data_dir();
    path.push(APP_ID);
    path
});

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SelectionState {
    start_line: i32,
    start_line_offset: i32,
    end_line: i32,
    end_line_offset: i32,
}

impl SelectionState {
    fn for_document(document: &Document) -> Self {
        let insert = document.get_insert();
        let start_iter = document.iter_at_mark(&insert);

        let bound = document.selection_bound();
        let end_iter = document.iter_at_mark(&bound);

        SelectionState {
            start_line: start_iter.line(),
            start_line_offset: start_iter.line_offset(),
            end_line: end_iter.line(),
            end_line_offset: end_iter.line_offset(),
        }
    }

    fn restore_on(&self, document: &Document) {
        let start = document.iter_at_line_offset(self.start_line, self.start_line_offset);
        let end = document.iter_at_line_offset(self.end_line, self.end_line_offset);

        match (start, end) {
            (Some(start), Some(end)) => {
                document.select_range(&start, &end);
            }
            _ => tracing::warn!("Failed to restore selection: missing start and end iters"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageState {
    paned_position: i32,
    is_active: bool,
    uri: Option<String>,
    selection: SelectionState,
    layout_engine: LayoutEngine,
}

impl PageState {
    pub fn for_page(page: &Page) -> Self {
        let document = page.document();

        Self {
            paned_position: page.paned_position(),
            is_active: page.is_active(),
            uri: document.file().map(|f| f.uri().into()),
            selection: SelectionState::for_document(&document),
            layout_engine: page.layout_engine(),
        }
    }

    pub fn restore_on(&self, page: &Page) {
        page.set_paned_position(self.paned_position);
        page.set_layout_engine(self.layout_engine);

        if let Some(uri) = &self.uri {
            let file = gio::File::for_uri(uri);
            utils::spawn(
                clone!(@weak page, @strong self.selection as selection_state  => async move {
                    if let Err(err) = page.load_file(file).await {
                        tracing::error!("Failed to load file for page: {:?}", err);
                    }

                    // Only restore selection once we have fully loaded the page's document.
                    let document = page.document();
                    selection_state.restore_on(&document);
                }),
            );
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct WindowState {
    width: i32,
    height: i32,
    is_maximized: bool,
    is_active: bool,
    pages: Vec<PageState>,
    closed_pages: Vec<PageState>,
}

impl WindowState {
    fn for_window(window: &Window) -> Self {
        let page_states = window
            .pages()
            .iter()
            .map(PageState::for_page)
            .collect::<Vec<_>>();

        WindowState {
            width: window.default_width(),
            height: window.default_height(),
            is_maximized: window.is_maximized(),
            is_active: window.is_active(),
            pages: page_states,
            closed_pages: window.closed_pages(),
        }
    }

    fn restore_on(&self, window: &Window) {
        window.set_default_size(self.width, self.height);
        window.set_maximized(self.is_maximized);
        window.set_closed_pages(self.closed_pages.clone());

        let mut active_page = None;
        for page_state in &self.pages {
            let page = window.add_new_page();
            page_state.restore_on(&page);

            if page_state.is_active {
                let prev_value = active_page.replace(page);
                debug_assert!(prev_value.is_none());
            }
        }

        if let Some(page) = active_page {
            window.set_selected_page(&page);
        }

        window.present();

        if self.pages.is_empty() {
            window.add_new_page();
        }
    }
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

    impl ObjectImpl for Session {}
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

        // If the window is the only window left, save the session before removing it.
        // Otherwise, remove it immediately.
        if matches!(self.imp().windows.borrow().as_slice(), [w] if w == window) {
            imp.default_window_width.set(window.default_width());
            imp.default_window_height.set(window.default_height());

            let app = Application::instance();
            let hold_guard = app.hold();

            utils::spawn(clone!(@weak self as obj, @weak window => async move {
                tracing::debug!("Saving session on last window");

                let _hold_guard = hold_guard;

                if let Err(err) = obj.save().await {
                    tracing::debug!("Failed to save session on last window: {:?}", err);
                }

                obj.remove_window_inner(&window);
            }));
        } else {
            self.remove_window_inner(window);
        }
    }

    pub fn open_files(&self, files: &[gio::File], window: &Window) {
        match files {
            [] => {
                tracing::warn!("Tried to open empty list of files");
            }
            [file] => {
                // If the document is already loaded in other windows or pages, just present it.
                for window in self.windows() {
                    for page in window.pages() {
                        if page
                            .document()
                            .file()
                            .is_some_and(|f| f.uri() == file.uri())
                        {
                            window.set_selected_page(&page);
                            window.present();

                            tracing::debug!("Shown file in an existing page");

                            return;
                        }
                    }
                }

                // Load the document in the current page if it is a draft and empty, otherwise
                // create a new page and load the document there.
                let page = match window.selected_page() {
                    Some(page) if page.document().is_safely_discardable() => page,
                    _ => window.add_new_page(),
                };
                utils::spawn(clone!(@weak window, @strong file => async move {
                    if let Err(err) = page.load_file(file).await {
                        tracing::error!("Failed to open file: {:?}", err);
                        window.add_message_toast(&gettext("Failed to open file"));
                    }
                }));
            }
            files => {
                // If there are many files, simply load them to new pages.
                for file in files {
                    utils::spawn(clone!(@weak window, @strong file => async move {
                        let page = window.add_new_page();
                        if let Err(err) = page.load_file(file).await {
                            tracing::error!("Failed to open file: {:?}", err);
                            window.add_message_toast(&gettext("Failed to open file"));
                        }
                    }));
                }
            }
        }

        window.present();
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
            window_state.restore_on(&window);

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

        tracing::debug!(
            elapsed = ?now.elapsed(),
            path = %APP_DATA_DIR.display(),
            ?state,
            "Session restored"
        );

        Ok(())
    }

    pub async fn save(&self) -> Result<()> {
        let imp = self.imp();

        let now = Instant::now();

        let window_states = imp
            .windows
            .borrow()
            .iter()
            .map(WindowState::for_window)
            .collect::<Vec<_>>();
        let state = State {
            windows: window_states,
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

        tracing::debug!(
            elapsed = ?now.elapsed(),
            path = %APP_DATA_DIR.display(),
            ?state,
            "Session saved"
        );

        Ok(())
    }

    fn remove_window_inner(&self, window: &Window) {
        let imp = self.imp();

        imp.windows.borrow_mut().retain(|w| w != window);
    }
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}
