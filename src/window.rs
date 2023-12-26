use adw::{prelude::*, subclass::prelude::*};
use anyhow::Result;
use gettextrs::gettext;
use gtk::{
    gdk, gio,
    glib::{self, clone},
};

use crate::{
    application::Application,
    config::APP_ID,
    export_format::ExportFormat,
    page::Page,
    save_changes_dialog,
    session::{PageState, Session},
    utils,
};

// TODO
// * Recent files
// * Find and replace
// * Session autosave
// * modified file on disk handling
// * Bird's eye view of graph
// * Full screen view of graph
// * Drag and drop on tabs
// * dot language server, hover info, color picker, autocompletion, snippets, renames, etc.

const PAGE_IS_MODIFIED_HANDLER_ID_KEY: &str = "dagger-page-is-modified-handler-id";

mod imp {
    use std::cell::{OnceCell, RefCell};

    use crate::drag_overlay::DragOverlay;

    use super::*;

    #[derive(Debug, Default, gtk::CompositeTemplate)]
    #[template(resource = "/io/github/seadve/Dagger/ui/window.ui")]
    pub struct Window {
        #[template_child]
        pub(super) toast_overlay: TemplateChild<adw::ToastOverlay>,
        #[template_child]
        pub(super) tab_overview: TemplateChild<adw::TabOverview>,
        #[template_child]
        pub(super) document_modified_status: TemplateChild<gtk::Label>,
        #[template_child]
        pub(super) document_title_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub(super) tab_button: TemplateChild<adw::TabButton>,
        #[template_child]
        pub(super) drag_overlay: TemplateChild<DragOverlay>,
        #[template_child]
        pub(super) stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub(super) empty_page: TemplateChild<adw::StatusPage>,
        #[template_child]
        pub(super) tab_view: TemplateChild<adw::TabView>,

        pub(super) inhibit_cookie: RefCell<Option<u32>>,
        pub(super) closed_pages: RefCell<Vec<PageState>>,
        pub(super) selected_page_signals: OnceCell<glib::SignalGroup>,
        pub(super) tab_view_close_page_handler_id: OnceCell<glib::SignalHandlerId>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Window {
        const NAME: &'static str = "DaggerWindow";
        type Type = super::Window;
        type ParentType = adw::ApplicationWindow;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();

            klass.install_action("win.new-document", None, |obj, _, _| {
                obj.add_new_page();
            });

            klass.install_action_async("win.open-document", None, |obj, _, _| async move {
                if let Err(err) = obj.open_document().await {
                    if !err
                        .downcast_ref::<glib::Error>()
                        .is_some_and(|error| error.matches(gtk::DialogError::Dismissed))
                    {
                        tracing::error!("Failed to open document: {:?}", err);
                        obj.add_message_toast(&gettext("Failed to open document"));
                    }
                }
            });

            klass.install_action_async("win.save-document", None, |obj, _, _| async move {
                let page = obj.selected_page().unwrap();
                debug_assert!(page.can_save());

                if let Err(err) = page.save_document().await {
                    if !err
                        .downcast_ref::<glib::Error>()
                        .is_some_and(|error| error.matches(gtk::DialogError::Dismissed))
                    {
                        tracing::error!("Failed to save document: {:?}", err);
                        obj.add_message_toast(&gettext("Failed to save document"));
                    }
                }
            });

            klass.install_action_async("win.save-document-as", None, |obj, _, _| async move {
                let page = obj.selected_page().unwrap();
                debug_assert!(page.can_save());

                if let Err(err) = page.save_document_as().await {
                    if !err
                        .downcast_ref::<glib::Error>()
                        .is_some_and(|error| error.matches(gtk::DialogError::Dismissed))
                    {
                        tracing::error!("Failed to save document as: {:?}", err);
                        obj.add_message_toast(&gettext("Failed to save document as"));
                    }
                }
            });

            klass.install_action_async(
                "win.discard-document-changes",
                None,
                |obj, _, _| async move {
                    let page = obj.selected_page().unwrap();
                    debug_assert!(page.can_discard_changes());

                    if let Err(err) = page.discard_changes().await {
                        tracing::error!("Failed to discard document changes: {:?}", err);
                        obj.add_message_toast(&gettext("Failed to discard document changes"));
                    }
                },
            );

            klass.install_action_async(
                "win.open-containing-folder",
                None,
                |obj, _, _| async move {
                    let page = obj.selected_page().unwrap();
                    debug_assert!(page.can_open_containing_folder());

                    if let Err(err) = page.open_containing_folder().await {
                        tracing::error!("Failed to open containing folder: {:?}", err);
                        obj.add_message_toast(&gettext("Failed to open containing folder"));
                    }
                },
            );

            klass.install_action_async("win.export-graph", Some("s"), |obj, _, arg| async move {
                let raw_format = arg.unwrap().get::<String>().unwrap();

                let format = match raw_format.as_str() {
                    "svg" => ExportFormat::Svg,
                    "png" => ExportFormat::Png,
                    "jpeg" => ExportFormat::Jpeg,
                    _ => unreachable!("unknown format `{}`", raw_format),
                };

                let page = obj.selected_page().unwrap();
                debug_assert!(page.can_export_graph());

                if let Err(err) = page.export_graph(format).await {
                    if !err
                        .downcast_ref::<glib::Error>()
                        .is_some_and(|error| error.matches(gtk::DialogError::Dismissed))
                    {
                        tracing::error!("Failed to export graph: {:?}", err);
                        obj.add_message_toast(&gettext("Failed to export graph"));
                    }
                }
            });

            klass.install_action("win.select-page", Some("i"), |obj, _, args| {
                let index = args.unwrap().get::<i32>().unwrap();

                if index < obj.n_pages() {
                    let page = obj.nth_page(index);
                    obj.set_selected_page(&page);
                }
            });

            klass.install_action("win.move-page-to-left", None, |obj, _, _| {
                let imp = obj.imp();
                if let Some(page) = obj.selected_page() {
                    let tab_page = imp.tab_view.page(&page);
                    imp.tab_view.reorder_backward(&tab_page);
                }
            });
            klass.install_action("win.move-page-to-right", None, |obj, _, _| {
                let imp = obj.imp();
                if let Some(page) = obj.selected_page() {
                    let tab_page = imp.tab_view.page(&page);
                    imp.tab_view.reorder_forward(&tab_page);
                }
            });
            klass.install_action("win.move-page-to-new-window", None, |obj, _, _| {
                let imp = obj.imp();
                if let Some(page) = obj.selected_page() {
                    let session = Session::instance();

                    let new_window = session.add_new_raw_window();
                    new_window.set_default_width(obj.default_width());
                    new_window.set_default_height(obj.default_height());
                    new_window.present();

                    let tab_page = imp.tab_view.page(&page);
                    imp.tab_view
                        .transfer_page(&tab_page, &new_window.imp().tab_view, 0);
                }
            });
            klass.install_action_async("win.close-other-pages", None, |obj, _, _| async move {
                if let Some(page) = obj.selected_page() {
                    let pages = obj.pages();
                    let pages_to_close =
                        pages.into_iter().filter(|p| p != &page).collect::<Vec<_>>();
                    if !pages_to_close.is_empty() {
                        obj.request_close_pages(&pages_to_close).await;
                    }
                }
            });
            klass.install_action_async("win.close-page", None, |obj, _, _| async move {
                if let Some(page) = obj.selected_page() {
                    obj.request_close_pages(&[page]).await;
                }
            });
            klass.install_action_async("win.close-page-or-window", None, |obj, _, _| async move {
                if let Some(page) = obj.selected_page() {
                    obj.request_close_pages(&[page]).await;
                } else {
                    obj.close();
                }
            });

            klass.install_action("win.undo-close-page", None, |obj, _, _| {
                obj.restore_closed_page();
            });

            klass.add_binding_action(
                gdk::Key::T,
                gdk::ModifierType::CONTROL_MASK,
                "win.new-document",
                None,
            );
            klass.add_binding_action(
                gdk::Key::O,
                gdk::ModifierType::CONTROL_MASK,
                "win.open-document",
                None,
            );
            klass.add_binding_action(
                gdk::Key::S,
                gdk::ModifierType::CONTROL_MASK,
                "win.save-document",
                None,
            );
            klass.add_binding_action(
                gdk::Key::S,
                gdk::ModifierType::CONTROL_MASK | gdk::ModifierType::SHIFT_MASK,
                "win.save-document-as",
                None,
            );

            klass.add_binding_action(
                gdk::Key::_1,
                gdk::ModifierType::CONTROL_MASK,
                "win.select-page",
                Some(&0.into()),
            );
            klass.add_binding_action(
                gdk::Key::_2,
                gdk::ModifierType::CONTROL_MASK,
                "win.select-page",
                Some(&1.into()),
            );
            klass.add_binding_action(
                gdk::Key::_3,
                gdk::ModifierType::CONTROL_MASK,
                "win.select-page",
                Some(&2.into()),
            );
            klass.add_binding_action(
                gdk::Key::_4,
                gdk::ModifierType::CONTROL_MASK,
                "win.select-page",
                Some(&3.into()),
            );
            klass.add_binding_action(
                gdk::Key::_5,
                gdk::ModifierType::CONTROL_MASK,
                "win.select-page",
                Some(&4.into()),
            );
            klass.add_binding_action(
                gdk::Key::_6,
                gdk::ModifierType::CONTROL_MASK,
                "win.select-page",
                Some(&5.into()),
            );
            klass.add_binding_action(
                gdk::Key::_7,
                gdk::ModifierType::CONTROL_MASK,
                "win.select-page",
                Some(&6.into()),
            );
            klass.add_binding_action(
                gdk::Key::_8,
                gdk::ModifierType::CONTROL_MASK,
                "win.select-page",
                Some(&7.into()),
            );
            klass.add_binding_action(
                gdk::Key::_9,
                gdk::ModifierType::CONTROL_MASK,
                "win.select-page",
                Some(&8.into()),
            );

            klass.add_binding_action(
                gdk::Key::Page_Up,
                gdk::ModifierType::CONTROL_MASK | gdk::ModifierType::SHIFT_MASK,
                "win.move-page-to-left",
                None,
            );
            klass.add_binding_action(
                gdk::Key::KP_Page_Up,
                gdk::ModifierType::CONTROL_MASK | gdk::ModifierType::SHIFT_MASK,
                "win.move-page-to-left",
                None,
            );
            klass.add_binding_action(
                gdk::Key::Page_Down,
                gdk::ModifierType::CONTROL_MASK | gdk::ModifierType::SHIFT_MASK,
                "win.move-page-to-right",
                None,
            );
            klass.add_binding_action(
                gdk::Key::KP_Page_Down,
                gdk::ModifierType::CONTROL_MASK | gdk::ModifierType::SHIFT_MASK,
                "win.move-page-to-right",
                None,
            );
            klass.add_binding_action(
                gdk::Key::N,
                gdk::ModifierType::CONTROL_MASK | gdk::ModifierType::SHIFT_MASK,
                "win.move-page-to-new-window",
                None,
            );
            klass.add_binding_action(
                gdk::Key::W,
                gdk::ModifierType::CONTROL_MASK,
                "win.close-page-or-window",
                None,
            );

            klass.add_binding_action(
                gdk::Key::T,
                gdk::ModifierType::CONTROL_MASK | gdk::ModifierType::SHIFT_MASK,
                "win.undo-close-page",
                None,
            );
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for Window {
        fn constructed(&self) {
            self.parent_constructed();

            let obj = self.obj();

            if utils::is_devel_profile() {
                obj.add_css_class("devel");
            }

            self.empty_page.set_icon_name(Some(APP_ID));

            let selected_page_signals = glib::SignalGroup::new::<Page>();
            selected_page_signals.connect_notify_local(
                Some("title"),
                clone!(@weak obj => move |_, _| {
                    obj.update_title();
                }),
            );
            selected_page_signals.connect_notify_local(
                Some("is-modified"),
                clone!(@weak obj => move |_, _| {
                    obj.update_modified_status();
                }),
            );
            selected_page_signals.connect_notify_local(
                Some("can-save"),
                clone!(@weak obj => move |_, _| {
                    obj.update_save_action();
                }),
            );
            selected_page_signals.connect_notify_local(
                Some("can-discard-changes"),
                clone!(@weak obj => move |_, _| {
                    obj.update_discard_changes_action();
                }),
            );
            selected_page_signals.connect_notify_local(
                Some("can-export-graph"),
                clone!(@weak obj => move |_, _| {
                    obj.update_export_graph_action();
                }),
            );
            selected_page_signals.connect_notify_local(
                Some("can-open-containing-folder"),
                clone!(@weak obj => move |_, _| {
                    obj.update_open_containing_folder_action();
                }),
            );
            self.selected_page_signals
                .set(selected_page_signals)
                .unwrap();

            let drop_target = gtk::DropTarget::builder()
                .propagation_phase(gtk::PropagationPhase::Capture)
                .actions(gdk::DragAction::COPY)
                .formats(&gdk::ContentFormats::for_type(gdk::FileList::static_type()))
                .build();
            drop_target.connect_drop(clone!(@weak obj => @default-panic, move |_, value, _, _| {
                obj.handle_drop(&value.get::<gdk::FileList>().unwrap())
            }));
            self.drag_overlay.set_target(Some(&drop_target));

            self.tab_overview
                .connect_create_tab(clone!(@weak obj => @default-panic, move |_| {
                    let imp = obj.imp();
                    let page = obj.add_new_page();
                    imp.tab_view.page(&page)
                }));

            self.tab_view
                .connect_selected_page_notify(clone!(@weak obj => move |_| {
                    obj.update_stack_page();
                    obj.update_selected_page_signals_target();
                }));
            self.tab_view
                .connect_create_window(clone!(@weak obj => @default-panic, move |_| {
                    let session = Session::instance();

                    let new_window = session.add_new_raw_window();
                    new_window.set_default_width(obj.default_width());
                    new_window.set_default_height(obj.default_height());
                    new_window.present();

                    let tab_view = new_window.imp().tab_view.get();
                    Some(tab_view)
                }));
            self.tab_view
                .connect_setup_menu(clone!(@weak obj => move |_, tab_page| {
                    if let Some(tab_page) = tab_page {
                        let page = tab_page.child().downcast::<Page>().unwrap();
                        obj.set_selected_page(&page);
                    }
                }));

            let tab_view_close_page_handler_id = self.tab_view.connect_close_page(
                clone!(@weak obj => @default-panic, move |_, tab_page| {
                    obj.handle_tab_view_close_page(tab_page).into()
                }),
            );
            self.tab_view_close_page_handler_id
                .set(tab_view_close_page_handler_id)
                .unwrap();

            self.tab_view
                .bind_property("n-pages", &*self.tab_button, "visible")
                .transform_to(|_, n_pages: i32| Some(n_pages > 0))
                .sync_create()
                .build();

            obj.update_stack_page();
            obj.update_selected_page_signals_target();
            obj.update_undo_close_page_action();
        }
    }

    impl WidgetImpl for Window {}
    impl WindowImpl for Window {
        fn close_request(&self) -> glib::Propagation {
            let obj = self.obj();

            let unsaved_documents = obj
                .pages()
                .iter()
                .map(|page| page.document())
                .filter(|document| document.is_modified())
                .collect::<Vec<_>>();

            if !unsaved_documents.is_empty() {
                utils::spawn(clone!(@weak obj => async move {
                    if save_changes_dialog::run(&obj, &unsaved_documents)
                        .await
                        .is_proceed()
                    {
                        let session = Session::instance();
                        session.remove_window(&obj);

                        obj.destroy();
                    }
                }));
                return glib::Propagation::Stop;
            }

            let session = Session::instance();
            session.remove_window(&obj);

            self.parent_close_request()
        }
    }

    impl ApplicationWindowImpl for Window {}
    impl AdwApplicationWindowImpl for Window {}
}

glib::wrapper! {
    pub struct Window(ObjectSubclass<imp::Window>)
        @extends gtk::Widget, gtk::Window, gtk::ApplicationWindow,
        @implements gio::ActionMap, gio::ActionGroup, gtk::Root;
}

impl Window {
    pub fn new(app: &Application) -> Self {
        glib::Object::builder().property("application", app).build()
    }

    pub fn add_toast(&self, toast: adw::Toast) {
        self.imp().toast_overlay.add_toast(toast);
    }

    pub fn add_message_toast(&self, message: &str) {
        self.add_toast(adw::Toast::new(message));
    }

    pub fn add_new_page(&self) -> Page {
        let imp = self.imp();

        let page = Page::new();
        page.set_paned_position(self.default_width() / 2);

        let tab_page = imp.tab_view.append(&page);
        page.bind_property("title", &tab_page, "title")
            .sync_create()
            .build();
        page.bind_property("is-busy", &tab_page, "loading")
            .sync_create()
            .build();
        page.bind_property("is-modified", &tab_page, "icon")
            .sync_create()
            .transform_to(|_, is_modified: bool| {
                let icon = if is_modified {
                    Some(gio::ThemedIcon::new("document-modified-symbolic"))
                } else {
                    None
                };
                Some(icon)
            })
            .build();

        unsafe {
            let is_modified_handler_id =
                page.connect_is_modified_notify(clone!(@weak self as obj => move |_| {
                    obj.update_inhibit();
                }));
            page.set_data(PAGE_IS_MODIFIED_HANDLER_ID_KEY, is_modified_handler_id);
        }

        self.update_inhibit();

        imp.tab_view.set_selected_page(&tab_page);

        page
    }

    pub async fn request_close_pages<'a>(&self, pages: &[Page]) {
        debug_assert!(!pages.is_empty());

        let imp = self.imp();

        let handler_id = imp.tab_view_close_page_handler_id.get().unwrap();

        // Block our handler, so it will immediately confirm closing the page
        // as we already handle here unsaved changes.
        imp.tab_view.block_signal(handler_id);

        let mut unsaved_pages = Vec::new();
        for page in pages {
            if !page.is_modified() {
                let tab_page = imp.tab_view.page(page);
                imp.tab_view.close_page(&tab_page);
                self.remove_page(page);
                continue;
            }

            unsaved_pages.push(page);
        }

        let unsaved_documents = unsaved_pages
            .iter()
            .map(|page| page.document())
            .collect::<Vec<_>>();
        if !unsaved_documents.is_empty()
            && save_changes_dialog::run(self, &unsaved_documents)
                .await
                .is_proceed()
        {
            for page in unsaved_pages {
                let tab_page = imp.tab_view.page(page);
                imp.tab_view.close_page(&tab_page);
                self.remove_page(page);
            }
        }

        imp.tab_view.unblock_signal(handler_id);
    }

    pub fn pages(&self) -> Vec<Page> {
        self.imp()
            .tab_view
            .pages()
            .upcast::<gio::ListModel>()
            .iter::<adw::TabPage>()
            .map(|tab_page| tab_page.unwrap().child().downcast().unwrap())
            .collect()
    }

    pub fn n_pages(&self) -> i32 {
        self.imp().tab_view.n_pages()
    }

    pub fn nth_page(&self, index: i32) -> Page {
        self.imp()
            .tab_view
            .nth_page(index)
            .child()
            .downcast()
            .unwrap()
    }

    pub fn selected_page(&self) -> Option<Page> {
        self.imp()
            .tab_view
            .selected_page()
            .map(|tab_page| tab_page.child().downcast().unwrap())
    }

    pub fn set_selected_page(&self, page: &Page) {
        let imp = self.imp();

        let tab_page = imp.tab_view.page(page);
        imp.tab_view.set_selected_page(&tab_page);
    }

    pub fn set_closed_pages(&self, page_states: Vec<PageState>) {
        let imp = self.imp();

        imp.closed_pages.replace(page_states);
        self.update_undo_close_page_action();
    }

    pub fn closed_pages(&self) -> Vec<PageState> {
        let imp = self.imp();

        imp.closed_pages.borrow().clone()
    }

    async fn open_document(&self) -> Result<()> {
        let dialog = gtk::FileDialog::builder()
            .title(gettext("Open Document"))
            .filters(&utils::graphviz_file_filters())
            .modal(true)
            .build();
        let file = dialog.open_future(Some(self)).await?;

        // Check if the document is already loaded in other windows or pages
        let session = Session::instance();
        for window in session.windows() {
            for page in window.pages() {
                if page
                    .document()
                    .file()
                    .is_some_and(|f| f.uri() == file.uri())
                {
                    window.set_selected_page(&page);
                    window.present();
                    return Ok(());
                }
            }
        }

        // Load the document in the current page if it is a draft and empty, otherwise
        // create a new page and load the document there.
        match self.selected_page() {
            Some(page) if page.document().is_safely_discardable() => {
                page.load_file(file).await?;
            }
            _ => {
                let page = self.add_new_page();
                page.load_file(file).await?;
            }
        }

        Ok(())
    }

    fn remove_page(&self, page: &Page) {
        let imp = self.imp();

        if !page.document().is_draft() {
            let page_state = PageState::for_page(page);
            tracing::debug!(?page_state, "Saved page state");

            imp.closed_pages.borrow_mut().push(page_state);
            self.update_undo_close_page_action();
        }

        unsafe {
            let is_modified_handler_id = page
                .steal_data::<glib::SignalHandlerId>(PAGE_IS_MODIFIED_HANDLER_ID_KEY)
                .unwrap();
            page.disconnect(is_modified_handler_id);
        }

        self.update_inhibit();
    }

    fn restore_closed_page(&self) {
        let imp = self.imp();

        let page_state = imp.closed_pages.borrow_mut().pop();
        if let Some(page_state) = page_state {
            let page = self.add_new_page();
            page_state.restore_on(&page);

            self.update_undo_close_page_action();
        }
    }

    fn handle_tab_view_close_page(&self, tab_page: &adw::TabPage) -> glib::Propagation {
        let page = tab_page.child().downcast::<Page>().unwrap();

        let document = page.document();
        if document.is_modified() {
            utils::spawn(clone!(@weak self as obj, @weak tab_page => async move {
                let imp = obj.imp();
                if save_changes_dialog::run(&obj, &[document]).await.is_proceed() {
                    imp.tab_view.close_page_finish(&tab_page, true);
                    obj.remove_page(&page);
                } else {
                    imp.tab_view.close_page_finish(&tab_page, false);
                }
            }));
            return glib::Propagation::Stop;
        }

        self.remove_page(&page);

        glib::Propagation::Proceed
    }

    fn handle_drop(&self, file_list: &gdk::FileList) -> bool {
        let files = file_list.files();

        if files.is_empty() {
            tracing::warn!("Given files is empty");
            return false;
        }

        utils::spawn(clone!(@weak self as obj => async move {
            obj.handle_drop_inner(files).await;
        }));

        true
    }

    async fn handle_drop_inner(&self, files: Vec<gio::File>) {
        for file in files {
            let page = self.add_new_page();

            if let Err(err) = page.load_file(file).await {
                tracing::error!("Failed to load file: {:?}", err);
                self.add_message_toast(&gettext("Failed to load file"));
            }
        }
    }

    fn update_inhibit(&self) {
        let imp = self.imp();

        let app = Application::instance();
        let has_modified = self.pages().iter().any(|page| page.is_modified());

        if has_modified && imp.inhibit_cookie.borrow().is_none() {
            let inhibit_cookie = app.inhibit(
                Some(self),
                gtk::ApplicationInhibitFlags::LOGOUT,
                Some(&gettext("There are unsaved documents")),
            );
            imp.inhibit_cookie.replace(Some(inhibit_cookie));

            tracing::debug!("Inhibited logout");
        } else if !has_modified {
            if let Some(inhibit_cookie) = imp.inhibit_cookie.take() {
                app.uninhibit(inhibit_cookie);

                tracing::debug!("Uninhibited logout");
            }
        }
    }

    fn update_stack_page(&self) {
        let imp = self.imp();

        if self.selected_page().is_some() {
            imp.stack.set_visible_child(&*imp.tab_view);
        } else {
            imp.stack.set_visible_child(&*imp.empty_page);
        }
    }

    fn update_selected_page_signals_target(&self) {
        let imp = self.imp();

        let selected_page_signals = imp.selected_page_signals.get().unwrap();
        selected_page_signals.set_target(self.selected_page().as_ref());

        self.update_title();
        self.update_modified_status();
        self.update_save_action();
        self.update_discard_changes_action();
        self.update_export_graph_action();
        self.update_open_containing_folder_action();
    }

    fn update_title(&self) {
        let imp = self.imp();

        let app_name = utils::application_name();

        let header_title = self
            .selected_page()
            .map_or_else(|| app_name.to_string(), |page| page.title());
        imp.document_title_label.set_text(&header_title);

        let window_title = self.selected_page().map_or_else(
            || app_name.to_string(),
            |page| format!("{} - {}", page.title(), app_name),
        );
        self.set_title(Some(&window_title));
    }

    fn update_modified_status(&self) {
        let imp = self.imp();
        let is_modified = self
            .selected_page()
            .map(|page| page.is_modified())
            .unwrap_or_default();
        imp.document_modified_status.set_visible(is_modified);
    }

    fn update_save_action(&self) {
        let can_save = self.selected_page().is_some_and(|page| page.can_save());
        self.action_set_enabled("win.save-document", can_save);
        self.action_set_enabled("win.save-document-as", can_save);
    }

    fn update_discard_changes_action(&self) {
        let can_discard_changes = self
            .selected_page()
            .is_some_and(|page| page.can_discard_changes());
        self.action_set_enabled("win.discard-document-changes", can_discard_changes);
    }

    fn update_export_graph_action(&self) {
        let can_export_graph = self
            .selected_page()
            .is_some_and(|page| page.can_export_graph());
        self.action_set_enabled("win.export-graph", can_export_graph);
    }

    fn update_open_containing_folder_action(&self) {
        let can_open_containing_folder = self
            .selected_page()
            .is_some_and(|page| page.can_open_containing_folder());
        self.action_set_enabled("win.open-containing-folder", can_open_containing_folder);
    }

    fn update_undo_close_page_action(&self) {
        let is_empty = self.imp().closed_pages.borrow().is_empty();
        self.action_set_enabled("win.undo-close-page", !is_empty);
    }
}
