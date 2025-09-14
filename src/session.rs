use std::time::Instant;

use anyhow::Result;
use gettextrs::gettext;
use gtk::{
    gio,
    glib::{self, clone},
    prelude::*,
    subclass::prelude::*,
};
use serde::{Deserialize, Serialize};

use crate::{
    APP_DATA_DIR, Application, document::Document, graph_view::LayoutEngine, page::Page,
    recent_list::RecentList, utils, window::Window,
};

const DEFAULT_WINDOW_WIDTH: i32 = 1000;
const DEFAULT_WINDOW_HEIGHT: i32 = 600;

const AUTO_SAVE_DELAY_SECS: u32 = 3;

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
            utils::spawn(clone!(
                #[weak]
                page,
                #[strong(rename_to = selection_state)]
                self.selection,
                async move {
                    if let Err(err) = page.load_file(file).await {
                        tracing::error!("Failed to load file for page: {:?}", err);
                        page.add_message_toast(&gettext("Failed to load file"));
                        return;
                    }

                    // Only restore selection once we have fully loaded the page's document.
                    let document = page.document();
                    selection_state.restore_on(&document);
                }
            ));
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
    default_window_width: i32,
    default_window_height: i32,
    windows: Vec<WindowState>,
}

mod imp {
    use std::cell::{Cell, RefCell};

    use async_lock::OnceCell;

    use super::*;

    pub struct Session {
        pub(super) state_file: gio::File,

        pub(super) default_window_width: Cell<i32>,
        pub(super) default_window_height: Cell<i32>,

        pub(super) windows: RefCell<Vec<Window>>,
        pub(super) recents: OnceCell<RecentList>,

        pub(super) is_dirty: Cell<bool>,
        pub(super) auto_save_source_id: RefCell<Option<glib::SourceId>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Session {
        const NAME: &'static str = "DelineateSession";
        type Type = super::Session;

        fn new() -> Self {
            Self {
                state_file: gio::File::for_path(APP_DATA_DIR.join("state.json")),
                default_window_width: Cell::new(DEFAULT_WINDOW_WIDTH),
                default_window_height: Cell::new(DEFAULT_WINDOW_HEIGHT),
                windows: RefCell::default(),
                recents: OnceCell::default(),
                is_dirty: Cell::default(),
                auto_save_source_id: RefCell::default(),
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
        Application::get().session().clone()
    }

    pub async fn recents(&self) -> &RecentList {
        let imp = self.imp();

        imp.recents
            .get_or_init(|| async {
                RecentList::load().await.unwrap_or_else(|err| {
                    tracing::error!("Failed to load recents: {:?}", err);
                    RecentList::new()
                })
            })
            .await
    }

    /// Returns the active window or creates a new one if there are no windows.
    pub fn active_window(&self) -> Window {
        let app = Application::get();

        if let Some(active_window) = app.active_window() {
            active_window.downcast::<Window>().unwrap()
        } else if let Some(window) = app.windows().first() {
            window.clone().downcast::<Window>().unwrap()
        } else {
            self.add_new_window()
        }
    }

    pub fn windows(&self) -> Vec<Window> {
        self.imp().windows.borrow().clone()
    }

    pub fn add_new_raw_window(&self) -> Window {
        let imp = self.imp();

        let app = Application::get();
        let window = Window::new(&app);

        let group = gtk::WindowGroup::new();
        group.add_window(&window);

        imp.windows.borrow_mut().push(window.clone());

        self.mark_dirty();

        window
    }

    pub fn add_new_window(&self) -> Window {
        let imp = self.imp();

        let window = self.add_new_raw_window();

        let default_width = imp.default_window_width.get();
        let default_height = imp.default_window_height.get();
        if default_width > 0 && default_height > 0 {
            window.set_default_size(default_width, default_height);
        } else {
            window.set_default_size(DEFAULT_WINDOW_WIDTH, DEFAULT_WINDOW_HEIGHT);
        };

        window.add_new_page();

        self.mark_dirty();

        window
    }

    pub fn remove_window(&self, window: &Window) {
        let imp = self.imp();

        // If the window is the only window left, save the session before removing it.
        // Otherwise, remove it immediately.
        if matches!(self.imp().windows.borrow().as_slice(), [w] if w == window) {
            imp.default_window_width.set(window.default_width());
            imp.default_window_height.set(window.default_height());

            let app = Application::get();
            let hold_guard = app.hold();

            utils::spawn(clone!(
                #[weak(rename_to = obj)]
                self,
                #[weak]
                window,
                async move {
                    tracing::debug!("Saving session on last window");

                    let _hold_guard = hold_guard;

                    if let Err(err) = obj.save().await {
                        tracing::debug!("Failed to save session on last window: {:?}", err);
                    }

                    obj.remove_window_inner(&window);
                }
            ));
        } else {
            self.remove_window_inner(window);
        }
    }

    fn remove_window_inner(&self, window: &Window) {
        let imp = self.imp();

        imp.windows.borrow_mut().retain(|w| w != window);

        self.mark_dirty();
    }

    pub fn open_files(&self, files: &[gio::File], window: &Window) {
        match files {
            [] => {
                tracing::error!("Tried to open empty list of files");
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
                self.load_file(&page, file.clone());
            }
            files => {
                // If there are many files, simply load them to new pages.
                for file in files {
                    let page = window.add_new_page();
                    self.load_file(&page, file.clone());
                }
            }
        }

        window.present();
    }

    pub async fn restore(&self) -> Result<()> {
        let imp = self.imp();

        let now = Instant::now();

        let state = match imp.state_file.load_bytes_future().await {
            Ok((bytes, _)) => serde_json::from_slice::<State>(&bytes)?,
            Err(err) => {
                if !err.matches(gio::IOErrorEnum::NotFound) {
                    return Err(err.into());
                }

                State::default()
            }
        };
        tracing::trace!(?state, "State loaded");

        imp.default_window_width.set(state.default_window_width);
        imp.default_window_height.set(state.default_window_height);

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

        tracing::debug!(elapsed = ?now.elapsed(), "Session restored");

        Ok(())
    }

    pub async fn save(&self) -> Result<()> {
        let imp = self.imp();

        imp.is_dirty.set(false);

        let now = Instant::now();

        let window_states = imp
            .windows
            .borrow()
            .iter()
            .map(WindowState::for_window)
            .collect::<Vec<_>>();
        let state = State {
            windows: window_states,
            default_window_width: imp.default_window_width.get(),
            default_window_height: imp.default_window_height.get(),
        };
        tracing::trace!(?state, "State stored");

        let bytes = serde_json::to_vec(&state)?;
        imp.state_file
            .replace_contents_future(
                bytes,
                None,
                false,
                gio::FileCreateFlags::REPLACE_DESTINATION,
            )
            .await
            .map_err(|(_, err)| err)?;

        self.recents().await.save().await?;

        tracing::debug!(elapsed = ?now.elapsed(), "Session saved");

        Ok(())
    }

    // FIXME Ideally, this should be an internal method and called when State fields change.
    pub fn mark_dirty(&self) {
        let imp = self.imp();

        if imp.is_dirty.get() {
            return;
        }

        imp.is_dirty.set(true);

        if let Some(source_id) = imp.auto_save_source_id.take() {
            source_id.remove();
        }

        let source_id = glib::timeout_add_seconds_local_once(
            AUTO_SAVE_DELAY_SECS,
            clone!(
                #[weak(rename_to = obj)]
                self,
                move || {
                    let _ = obj.imp().auto_save_source_id.take();

                    utils::spawn(async move {
                        tracing::debug!("Saving session on auto save");

                        if let Err(err) = obj.save().await {
                            tracing::debug!("Failed to save session on auto save: {:?}", err);
                        }
                    });
                }
            ),
        );
        imp.auto_save_source_id.replace(Some(source_id));
    }

    fn load_file(&self, page: &Page, file: gio::File) {
        utils::spawn(clone!(
            #[weak(rename_to = obj)]
            self,
            #[weak]
            page,
            async move {
                // Add to recents immediately, so huge files won't be delayed in being added.
                obj.recents().await.add(file.uri().to_string());

                if let Err(err) = page.load_file(file).await {
                    tracing::error!("Failed to open file: {:?}", err);
                    page.add_message_toast(&gettext("Failed to open file"));
                }

                obj.mark_dirty();
            }
        ));
    }
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}
