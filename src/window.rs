use adw::{prelude::*, subclass::prelude::*};
use anyhow::Result;
use gettextrs::gettext;
use gtk::{
    gdk, gio,
    glib::{self, clone},
};

use crate::{
    application::Application, config::APP_ID, export_format::ExportFormat, page::Page,
    session::Session, utils,
};

// TODO
// * Find and replace
// * Bird's eye view of graph
// * Full screen view of graph
// * Recent files
// * Drag and drop on tabs
// * Inhibit when has unsaved changes
// * dot language server, hover info, color picker, autocompletion, snippets, renames, etc.
// * modified file on disk handling

// FIXME
// * Session saving (unsaved documents, etc.)
// * Shortcuts

mod imp {
    use std::cell::OnceCell;

    use crate::drag_overlay::DragOverlay;

    use super::*;

    #[derive(Debug, Default, gtk::CompositeTemplate)]
    #[template(resource = "/io/github/seadve/Dagger/ui/window.ui")]
    pub struct Window {
        #[template_child]
        pub(super) toast_overlay: TemplateChild<adw::ToastOverlay>,
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
        pub(super) empty_page_description: TemplateChild<gtk::Label>,
        #[template_child]
        pub(super) tab_view: TemplateChild<adw::TabView>,

        pub(super) page_signal_group: OnceCell<glib::SignalGroup>,
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

            klass.install_action_async("win.export-graph", Some("s"), |obj, _, arg| async move {
                let raw_format = arg.unwrap().get::<String>().unwrap();

                let format = match raw_format.as_str() {
                    "svg" => ExportFormat::Svg,
                    "png" => ExportFormat::Png,
                    "jpeg" => ExportFormat::Jpeg,
                    _ => unreachable!("unknown format `{}`", raw_format),
                };

                let page = obj.selected_page().unwrap();
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

            klass.install_action("win.close-page-or-window", None, |obj, _, _| {
                if let Some(selected_page) = obj.selected_page() {
                    obj.close_page(&selected_page);
                } else {
                    obj.close();
                }
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
                gdk::Key::W,
                gdk::ModifierType::CONTROL_MASK,
                "win.close-page-or-window",
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
            self.empty_page_description.set_label(&gettext(
                "• Press the Open button
• Press the New Tab Button
• Press Ctrl+N to start a new document
• Press Ctrl+O to browse for a document
• Drag a file into the window

Or, press Ctrl+W to close the window.",
            ));

            let page_signal_group = glib::SignalGroup::new::<Page>();
            page_signal_group.connect_notify_local(
                Some("title"),
                clone!(@weak obj => move |_, _| {
                    obj.update_title();
                }),
            );
            page_signal_group.connect_notify_local(
                Some("is-modified"),
                clone!(@weak obj => move |_, _| {
                    obj.update_is_modified();
                }),
            );
            page_signal_group.connect_notify_local(
                Some("can-save"),
                clone!(@weak obj => move |_, _| {
                    obj.update_save_action();
                }),
            );
            page_signal_group.connect_notify_local(
                Some("can-export"),
                clone!(@weak obj => move |_, _| {
                    obj.update_export_graph_action();
                }),
            );
            self.page_signal_group.set(page_signal_group).unwrap();

            let drop_target = gtk::DropTarget::builder()
                .propagation_phase(gtk::PropagationPhase::Capture)
                .actions(gdk::DragAction::COPY)
                .formats(&gdk::ContentFormats::for_type(gdk::FileList::static_type()))
                .build();
            drop_target.connect_drop(clone!(@weak obj => @default-panic, move |_, value, _, _| {
                obj.handle_drop(&value.get::<gdk::FileList>().unwrap())
            }));
            self.drag_overlay.set_target(Some(&drop_target));

            self.tab_view
                .connect_selected_page_notify(clone!(@weak obj => move |_| {
                    obj.update_stack_page();
                    obj.bind_page(obj.selected_page().as_ref());
                }));
            self.tab_view
                .connect_create_window(clone!(@weak obj => @default-panic, move |_| {
                    let session = Session::instance();

                    let window = session.add_new_raw_window();
                    window.set_default_width(obj.default_width());
                    window.set_default_height(obj.default_height());
                    window.present();

                    let tab_view = window.imp().tab_view.get();
                    Some(tab_view)
                }));

            self.tab_view
                .bind_property("n-pages", &*self.tab_button, "visible")
                .transform_to(|_, n_pages: i32| Some(n_pages > 0))
                .sync_create()
                .build();

            obj.update_stack_page();
            obj.bind_page(None);
        }

        fn dispose(&self) {
            self.dispose_template();

            let obj = self.obj();

            let session = Session::instance();
            session.remove_window(&obj);
        }
    }

    impl WidgetImpl for Window {}
    impl WindowImpl for Window {
        fn close_request(&self) -> glib::Propagation {
            let obj = self.obj();

            let session = Session::instance();
            session.remove_window(&obj);

            // let prev_document = obj.document();
            // if prev_document.is_modified() {
            //     utils::spawn(
            //         glib::Priority::default(),
            //         clone!(@weak obj => async move {
            //             if obj.handle_unsaved_changes(&prev_document).await.is_err() {
            //                 return;
            //             }
            //             obj.destroy();
            //         }),
            //     );
            //     return glib::Propagation::Stop;
            // }

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
        page.set_paned_position(self.width() / 2);

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

        imp.tab_view.set_selected_page(&tab_page);

        page
    }

    pub fn close_page(&self, page: &Page) {
        let imp = self.imp();

        let tab_page = imp.tab_view.page(page);
        imp.tab_view.close_page(&tab_page);
    }

    pub fn pages(&self) -> Vec<Page> {
        self.imp()
            .tab_view
            .pages()
            .upcast::<gio::ListModel>()
            .iter::<adw::TabPage>()
            .map(|tab_page| tab_page.unwrap().child().downcast::<Page>().unwrap())
            .collect()
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

        match self.selected_page() {
            Some(page) if page.document().is_discardable() => {
                page.load_file(file).await?;
            }
            _ => {
                let page = self.add_new_page();
                page.load_file(file).await?;
            }
        }

        Ok(())
    }

    fn bind_page(&self, page: Option<&Page>) {
        let imp = self.imp();

        let page_signal_group = imp.page_signal_group.get().unwrap();
        page_signal_group.set_target(page);

        self.update_title();
        self.update_is_modified();
        self.update_save_action();
        self.update_export_graph_action();
    }

    fn handle_drop(&self, file_list: &gdk::FileList) -> bool {
        let files = file_list.files();

        if files.is_empty() {
            tracing::warn!("Given files is empty");
            return false;
        }

        utils::spawn(
            glib::Priority::default(),
            clone!(@weak self as obj => async move {
                obj.handle_drop_inner(files).await;
            }),
        );

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

    fn update_stack_page(&self) {
        let imp = self.imp();

        if self.selected_page().is_some() {
            imp.stack.set_visible_child(&*imp.tab_view);
        } else {
            imp.stack.set_visible_child(&*imp.empty_page);
        }
    }

    fn update_title(&self) {
        let imp = self.imp();

        let app_name = glib::application_name().unwrap();

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

    fn update_is_modified(&self) {
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

    fn update_export_graph_action(&self) {
        let can_export = self.selected_page().is_some_and(|page| page.can_export());
        self.action_set_enabled("win.export-graph", can_export);
    }
}
