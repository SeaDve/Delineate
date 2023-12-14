use std::{error, fmt, time::Duration};

use adw::{prelude::*, subclass::prelude::*};
use anyhow::{Context, Result};
use gettextrs::gettext;
use gtk::{
    gdk, gdk_pixbuf, gio,
    glib::{self, clone, closure, once_cell::sync::Lazy},
};
use gtk_source::prelude::*;
use regex::Regex;

use crate::{
    application::Application,
    config::PROFILE,
    document::Document,
    drag_overlay::DragOverlay,
    error_gutter_renderer::ErrorGutterRenderer,
    graph_view::{Engine, GraphView},
    i18n::gettext_f,
    utils,
};

// TODO
// * Find and replace
// * Bird's eye view of graph
// * Full screen view of graph
// * Tabs and/or multiple windows
// * Recent files
// * dot language server, with error handling on text view
// * modified file on disk handling

const DRAW_GRAPH_INTERVAL: Duration = Duration::from_secs(1);

static ERROR_MESSAGE_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"syntax error in line (\d+)").expect("Failed to compile regex"));

#[derive(Debug, Clone, Copy)]
pub enum Format {
    Svg,
    Png,
    Jpeg,
}

impl Format {
    pub fn extension(&self) -> &'static str {
        match self {
            Self::Svg => "svg",
            Self::Png => "png",
            Self::Jpeg => "jpg",
        }
    }

    pub fn mime_type(&self) -> &'static str {
        match self {
            Self::Svg => "image/svg+xml",
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
        }
    }

    pub fn name(&self) -> String {
        match self {
            Self::Svg => gettext("SVG"),
            Self::Png => gettext("PNG"),
            Self::Jpeg => gettext("JPEG"),
        }
    }
}

/// Indicates that a task was cancelled.
#[derive(Debug)]
struct Cancelled;

impl fmt::Display for Cancelled {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Task cancelled")
    }
}

impl error::Error for Cancelled {}

mod imp {
    use std::cell::{Cell, OnceCell, RefCell};

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
        pub(super) drag_overlay: TemplateChild<DragOverlay>,
        #[template_child]
        pub(super) paned: TemplateChild<gtk::Paned>,
        #[template_child]
        pub(super) progress_bar: TemplateChild<gtk::ProgressBar>,
        #[template_child]
        pub(super) go_to_error_revealer: TemplateChild<gtk::Revealer>,
        #[template_child]
        pub(super) view: TemplateChild<gtk_source::View>,
        #[template_child]
        pub(super) graph_view: TemplateChild<GraphView>,
        #[template_child]
        pub(super) engine_drop_down: TemplateChild<gtk::DropDown>,
        #[template_child]
        pub(super) zoom_level_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub(super) spinner_revealer: TemplateChild<gtk::Revealer>,

        pub(super) error_gutter_renderer: ErrorGutterRenderer,
        pub(super) line_with_error: Cell<Option<u32>>,

        pub(super) document_binding_group: glib::BindingGroup,
        pub(super) document_signal_group: OnceCell<glib::SignalGroup>,

        pub(super) queued_draw_graph: Cell<bool>,
        pub(super) draw_graph_timeout_cancellable: RefCell<Option<gio::Cancellable>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Window {
        const NAME: &'static str = "DaggerWindow";
        type Type = super::Window;
        type ParentType = adw::ApplicationWindow;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();

            klass.install_action_async("win.new-document", None, |obj, _, _| async move {
                if obj.handle_unsaved_changes(&obj.document()).await.is_err() {
                    return;
                }

                obj.set_document(&Document::draft());
            });

            klass.install_action_async("win.open-document", None, |obj, _, _| async move {
                if obj.handle_unsaved_changes(&obj.document()).await.is_err() {
                    return;
                }

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
                if let Err(err) = obj.save_document(&obj.document()).await {
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
                if let Err(err) = obj.save_document_as(&obj.document()).await {
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
                    "svg" => Format::Svg,
                    "png" => Format::Png,
                    "jpeg" => Format::Jpeg,
                    _ => unreachable!("unknown format `{}`", raw_format),
                };

                if let Err(err) = obj.export_graph(format).await {
                    if !err
                        .downcast_ref::<glib::Error>()
                        .is_some_and(|error| error.matches(gtk::DialogError::Dismissed))
                    {
                        tracing::error!("Failed to export graph: {:?}", err);
                        obj.add_message_toast(&gettext("Failed to export graph"));
                    }
                } else {
                    obj.add_message_toast(&gettext("Graph exported"));
                }
            });

            klass.install_action("win.go-to-error", None, |obj, _, _| {
                let imp = obj.imp();

                let line = imp.line_with_error.get().unwrap();
                let mut iter = imp.view.buffer().iter_at_line(line as i32).unwrap();
                imp.view.scroll_to_iter(&mut iter, 0.0, true, 0.0, 0.5);
            });

            klass.install_action_async("win.zoom-graph-in", None, |obj, _, _| async move {
                if let Err(err) = obj.imp().graph_view.zoom_in().await {
                    tracing::error!("Failed to zoom in: {:?}", err);
                }
            });

            klass.install_action_async("win.zoom-graph-out", None, |obj, _, _| async move {
                if let Err(err) = obj.imp().graph_view.zoom_out().await {
                    tracing::error!("Failed to zoom out: {:?}", err);
                }
            });

            klass.install_action_async("win.reset-graph-zoom", None, |obj, _, _| async move {
                if let Err(err) = obj.imp().graph_view.reset_zoom().await {
                    tracing::error!("Failed to reset zoom: {:?}", err);
                }
            });
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for Window {
        fn constructed(&self) {
            self.parent_constructed();

            let obj = self.obj();

            if PROFILE == "Devel" {
                obj.add_css_class("devel");
            }

            self.document_binding_group
                .bind("is-modified", &*self.document_modified_status, "visible")
                .sync_create()
                .build();
            self.document_binding_group
                .bind("title", &*self.document_title_label, "label")
                .transform_to(|_, value| {
                    let title = value.get::<String>().unwrap();
                    let label = if title.is_empty() {
                        gettext("Untitled Document")
                    } else {
                        title
                    };
                    Some(label.into())
                })
                .sync_create()
                .build();
            self.document_binding_group
                .bind("busy-progress", &*self.progress_bar, "fraction")
                .sync_create()
                .build();
            self.document_binding_group
                .bind("busy-progress", &*self.progress_bar, "visible")
                .transform_to(|_, value| {
                    let busy_progress = value.get::<f64>().unwrap();
                    let visible = busy_progress != 1.0;
                    Some(visible.into())
                })
                .sync_create()
                .build();

            let document_signal_group = glib::SignalGroup::new::<Document>();
            document_signal_group.connect_local(
                "text-changed",
                false,
                clone!(@weak obj => @default-panic, move |_| {
                    obj.handle_document_text_changed();
                    None
                }),
            );
            document_signal_group.connect_notify_local(
                Some("loading"),
                clone!(@weak obj => move |_, _| {
                    obj.update_save_action();
                }),
            );
            self.document_signal_group
                .set(document_signal_group)
                .unwrap();

            self.engine_drop_down.set_expression(Some(
                &gtk::ClosureExpression::new::<glib::GString>(
                    &[] as &[gtk::Expression],
                    closure!(|list_item: adw::EnumListItem| list_item.name()),
                ),
            ));
            self.engine_drop_down
                .set_model(Some(&adw::EnumListModel::new(Engine::static_type())));
            self.engine_drop_down
                .connect_selected_notify(clone!(@weak obj => move |_| {
                    obj.queue_draw_graph();
                }));

            let drop_target = gtk::DropTarget::builder()
                .propagation_phase(gtk::PropagationPhase::Capture)
                .actions(gdk::DragAction::COPY)
                .formats(&gdk::ContentFormats::for_type(gdk::FileList::static_type()))
                .build();
            drop_target.connect_drop(clone!(@weak obj => @default-panic, move |_, value, _, _| {
                obj.handle_drop(&value.get::<gdk::FileList>().unwrap())
            }));
            self.drag_overlay.set_target(Some(&drop_target));

            let gutter = ViewExt::gutter(&*self.view, gtk::TextWindowType::Left);
            let was_inserted = gutter.insert(&self.error_gutter_renderer, 0);
            debug_assert!(was_inserted);

            self.go_to_error_revealer
                .connect_child_revealed_notify(|revealer| {
                    if !revealer.is_child_revealed() {
                        revealer.set_visible(false);
                    }
                });

            self.graph_view
                .connect_is_graph_loaded_notify(clone!(@weak obj => move |_| {
                    obj.update_export_graph_action();
                }));
            self.graph_view
                .connect_error(clone!(@weak obj => move |_, message| {
                    obj.handle_graph_view_error(message);
                }));
            self.graph_view
                .connect_is_rendering_notify(clone!(@weak obj => move |graph_view| {
                    if !graph_view.is_rendering() {
                        obj.imp().spinner_revealer.set_reveal_child(false);
                    }
                }));
            self.graph_view
                .connect_zoom_level_notify(clone!(@weak obj => move |_| {
                    obj.update_zoom_level_button();
                }));
            self.graph_view
                .connect_can_zoom_in_notify(clone!(@weak obj => move |_| {
                    obj.update_zoom_in_action();
                }));
            self.graph_view
                .connect_can_zoom_out_notify(clone!(@weak obj => move |_| {
                    obj.update_zoom_out_action();
                }));
            self.graph_view
                .connect_can_reset_zoom_notify(clone!(@weak obj => move |_| {
                    obj.update_reset_zoom_action();
                }));

            utils::spawn(
                glib::Priority::DEFAULT_IDLE,
                clone!(@weak obj => async move {
                    obj.start_draw_graph_loop().await;
                }),
            );

            obj.set_document(&Document::draft());
            obj.update_export_graph_action();
            obj.update_zoom_level_button();
            obj.update_zoom_in_action();
            obj.update_zoom_out_action();
            obj.update_reset_zoom_action();

            obj.load_window_state();
        }

        fn dispose(&self) {
            self.dispose_template();
        }
    }

    impl WidgetImpl for Window {}
    impl WindowImpl for Window {
        fn close_request(&self) -> glib::Propagation {
            let obj = self.obj();

            if let Err(err) = obj.save_window_state() {
                tracing::warn!("Failed to save window state, {}", &err);
            }

            let prev_document = obj.document();
            if prev_document.is_modified() {
                utils::spawn(
                    glib::Priority::default(),
                    clone!(@weak obj => async move {
                        if obj.handle_unsaved_changes(&prev_document).await.is_err() {
                            return;
                        }
                        obj.destroy();
                    }),
                );
                return glib::Propagation::Stop;
            }

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

    fn set_document(&self, document: &Document) {
        let imp = self.imp();

        imp.view.set_buffer(Some(document));

        imp.document_binding_group.set_source(Some(document));

        let document_signal_group = imp.document_signal_group.get().unwrap();
        document_signal_group.set_target(Some(document));
    }

    fn document(&self) -> Document {
        self.imp().view.buffer().downcast().unwrap()
    }

    fn selected_engine(&self) -> Engine {
        let imp = self.imp();
        let selected_item = imp
            .engine_drop_down
            .selected_item()
            .unwrap()
            .downcast::<adw::EnumListItem>()
            .unwrap();
        Engine::try_from(selected_item.value()).unwrap()
    }

    fn add_message_toast(&self, message: &str) {
        let toast = adw::Toast::new(message);
        self.imp().toast_overlay.add_toast(toast);
    }

    async fn load_file(&self, file: gio::File) -> Result<()> {
        let document = Document::for_file(file);
        let prev_document = self.document();
        self.set_document(&document);

        if let Err(err) = document.load().await {
            self.set_document(&prev_document);
            return Err(err);
        }

        Ok(())
    }

    async fn open_document(&self) -> Result<()> {
        let dialog = gtk::FileDialog::builder()
            .title(gettext("Open Document"))
            .filters(&graphviz_file_filters())
            .modal(true)
            .build();
        let file = dialog.open_future(Some(self)).await?;

        self.load_file(file).await?;

        Ok(())
    }

    async fn save_document(&self, document: &Document) -> Result<()> {
        if document.file().is_some() {
            document.save().await?;
        } else {
            let dialog = gtk::FileDialog::builder()
                .title(gettext("Save Document"))
                .filters(&graphviz_file_filters())
                .modal(true)
                .initial_name(format!("{}.gv", document.title()))
                .build();
            let file = dialog.save_future(Some(self)).await?;

            document.save_as(&file).await?;
        }

        Ok(())
    }

    async fn save_document_as(&self, document: &Document) -> Result<()> {
        let dialog = gtk::FileDialog::builder()
            .title(gettext("Save Document As"))
            .filters(&graphviz_file_filters())
            .modal(true)
            .initial_name(format!("{}.gv", document.title()))
            .build();
        let file = dialog.save_future(Some(self)).await?;

        document.save_as(&file).await?;

        Ok(())
    }

    async fn export_graph(&self, format: Format) -> Result<()> {
        let imp = self.imp();

        let filter = gtk::FileFilter::new();
        filter.set_name(Some(&format.name()));
        filter.add_mime_type(format.mime_type());
        filter.add_suffix(format.extension());

        let filters = gio::ListStore::new::<gtk::FileFilter>();
        filters.append(&filter);

        let document = self.document();

        let dialog = gtk::FileDialog::builder()
            .title(gettext("Export Graph"))
            .accept_label(gettext("_Export"))
            .initial_name(format!("{}.{}", document.title(), format.extension()))
            .filters(&filters)
            .modal(true)
            .build();
        let file = dialog.save_future(Some(self)).await?;

        let svg_bytes = imp
            .graph_view
            .get_svg()
            .await?
            .context("Failed to get SVG")?;

        let bytes = match format {
            Format::Svg => svg_bytes,
            Format::Png | Format::Jpeg => {
                let loader = gdk_pixbuf::PixbufLoader::new();
                loader
                    .write_bytes(&svg_bytes)
                    .context("Failed to write SVG bytes")?;
                loader.close().context("Failed to close loader")?;
                let pixbuf = loader.pixbuf().context("Loader has no pixbuf")?;

                let pixbuf_type = match format {
                    Format::Png => "png",
                    Format::Jpeg => "jpeg",
                    Format::Svg => unreachable!(),
                };
                let buffer = pixbuf.save_to_bufferv(pixbuf_type, &[])?;

                glib::Bytes::from_owned(buffer)
            }
        };

        file.replace_contents_future(
            bytes,
            None,
            false,
            gio::FileCreateFlags::REPLACE_DESTINATION,
        )
        .await
        .map_err(|(_, err)| err)?;

        tracing::debug!(uri = %file.uri(), "Graph exported");

        Ok(())
    }

    fn handle_drop(&self, file_list: &gdk::FileList) -> bool {
        let files = file_list.files();

        if files.is_empty() {
            tracing::warn!("Given files is empty");
            return false;
        }

        // TODO Support multiple files
        if files.len() > 1 {
            tracing::warn!("Multiple files dropped is not yet supported");
        }

        utils::spawn(
            glib::Priority::default(),
            clone!(@weak self as obj => async move {
                if obj.handle_unsaved_changes(&obj.document()).await.is_err() {
                    return ;
                }

                let file = files.get(0).unwrap();

                if let Err(err) = obj.load_file(file.clone()).await {
                    tracing::error!("Failed to load file: {:?}", err);
                    obj.add_message_toast(&gettext("Failed to load file"));
                }
            }),
        );

        true
    }

    fn handle_document_text_changed(&self) {
        let imp = self.imp();

        imp.error_gutter_renderer.clear_errors();

        imp.line_with_error.set(None);
        self.update_go_to_error_revealer();

        self.queue_draw_graph();
    }

    fn handle_graph_view_error(&self, message: &str) {
        let imp = self.imp();

        if let Some(captures) = ERROR_MESSAGE_REGEX.captures(message) {
            tracing::debug!("Syntax error: {}", message);

            let raw_line_number = captures[1]
                .parse::<u32>()
                .expect("Failed to parse line number");
            let line_number = raw_line_number - 1;
            imp.error_gutter_renderer
                .set_error(line_number, message.trim());

            // FIXME Show only when line is not visible
            imp.line_with_error.set(Some(line_number));
            self.update_go_to_error_revealer();
        } else {
            tracing::error!("Failed to draw graph: {}", message);

            self.add_message_toast(&gettext("Failed to draw graph"));
        }
    }

    /// Returns `Ok` if unsaved changes are handled and can proceed, `Err` if
    /// the next operation should be aborted.
    async fn handle_unsaved_changes(&self, document: &Document) -> Result<()> {
        if !document.is_modified() {
            return Ok(());
        }

        match self.present_save_changes_dialog(document).await {
            Ok(_) => Ok(()),
            Err(err) => {
                if !err.is::<Cancelled>()
                    && !err
                        .downcast_ref::<glib::Error>()
                        .is_some_and(|error| error.matches(gtk::DialogError::Dismissed))
                {
                    tracing::error!("Failed to save changes to document: {:?}", err);
                    self.add_message_toast(&gettext("Failed to save changes to document"));
                }
                Err(err)
            }
        }
    }

    /// Returns `Ok` if unsaved changes are handled and can proceed, `Err` if
    /// the next operation should be aborted.
    async fn present_save_changes_dialog(&self, document: &Document) -> Result<()> {
        const CANCEL_RESPONSE_ID: &str = "cancel";
        const DISCARD_RESPONSE_ID: &str = "discard";
        const SAVE_RESPONSE_ID: &str = "save";

        let file_name = document
            .file()
            .and_then(|file| {
                file.path()
                    .unwrap()
                    .file_name()
                    .map(|file_name| file_name.to_string_lossy().to_string())
            })
            .unwrap_or_else(|| gettext("Untitled Document"));
        let dialog = adw::MessageDialog::builder()
            .modal(true)
            .transient_for(self)
            .heading(gettext("Save Changes?"))
            .body(gettext_f(
                // Translators: Do NOT translate the contents between '{' and '}', this is a variable name.
                "“{file_name}” contains unsaved changes. Changes which are not saved will be permanently lost.",
                &[("file_name", &file_name)],
            ))
            .close_response(CANCEL_RESPONSE_ID)
            .default_response(SAVE_RESPONSE_ID)
            .build();

        dialog.add_response(CANCEL_RESPONSE_ID, &gettext("Cancel"));

        dialog.add_response(DISCARD_RESPONSE_ID, &gettext("Discard"));
        dialog.set_response_appearance(DISCARD_RESPONSE_ID, adw::ResponseAppearance::Destructive);

        let save_response_text = if document.file().is_some() {
            gettext("Save")
        } else {
            gettext("Save As…")
        };
        dialog.add_response(SAVE_RESPONSE_ID, &save_response_text);
        dialog.set_response_appearance(SAVE_RESPONSE_ID, adw::ResponseAppearance::Suggested);

        match dialog.choose_future().await.as_str() {
            CANCEL_RESPONSE_ID => Err(Cancelled.into()),
            DISCARD_RESPONSE_ID => Ok(()),
            SAVE_RESPONSE_ID => self.save_document(document).await,
            _ => unreachable!(),
        }
    }

    fn save_window_state(&self) -> Result<(), glib::BoolError> {
        let imp = self.imp();

        let app = utils::app_instance();
        let settings = app.settings();

        let (width, height) = self.default_size();

        settings.try_set_window_width(width)?;
        settings.try_set_window_height(height)?;
        settings.try_set_is_maximized(self.is_maximized())?;

        settings.try_set_paned_position(imp.paned.position())?;
        settings.try_set_layout_engine(self.selected_engine())?;

        Ok(())
    }

    fn load_window_state(&self) {
        let imp = self.imp();

        let app = utils::app_instance();
        let settings = app.settings();

        self.set_default_size(settings.window_width(), settings.window_height());

        if settings.is_maximized() {
            self.maximize();
        }

        imp.paned.set_position(settings.paned_position());
        imp.engine_drop_down
            .set_selected(settings.layout_engine() as u32);
    }

    fn queue_draw_graph(&self) {
        let imp = self.imp();

        imp.queued_draw_graph.set(true);

        // If we're not processing a graph, skip the timeout.
        if !imp.spinner_revealer.reveals_child() {
            if let Some(cancellable) = imp.draw_graph_timeout_cancellable.take() {
                cancellable.cancel();
            }
        }

        imp.spinner_revealer.set_reveal_child(true);
    }

    async fn start_draw_graph_loop(&self) {
        let imp = self.imp();

        loop {
            let cancellable = gio::Cancellable::new();
            let timeout = gio::CancellableFuture::new(
                glib::timeout_future(DRAW_GRAPH_INTERVAL),
                cancellable.clone(),
            );
            imp.draw_graph_timeout_cancellable
                .replace(Some(cancellable));

            let _ = timeout.await;

            if !imp.queued_draw_graph.get() {
                continue;
            }

            imp.queued_draw_graph.set(false);

            if let Err(err) = imp
                .graph_view
                .set_data(&self.document().contents(), self.selected_engine())
                .await
            {
                tracing::error!("Failed to render: {:?}", err);
            }
        }
    }

    fn update_go_to_error_revealer(&self) {
        let imp = self.imp();

        if imp.line_with_error.get().is_some() {
            imp.go_to_error_revealer.set_visible(true);
            imp.go_to_error_revealer.set_reveal_child(true);
        } else {
            imp.go_to_error_revealer.set_reveal_child(false);
        }
    }

    fn update_save_action(&self) {
        let is_loading = self.document().is_loading();
        self.action_set_enabled("win.save-document", !is_loading);
        self.action_set_enabled("win.save-document-as", !is_loading);
    }

    fn update_export_graph_action(&self) {
        let imp = self.imp();

        self.action_set_enabled("win.export-graph", imp.graph_view.is_graph_loaded());
    }

    fn update_zoom_level_button(&self) {
        let imp = self.imp();

        let zoom_level = imp.graph_view.zoom_level();
        imp.zoom_level_button
            .set_label(&format!("{:.0}%", zoom_level * 100.0));
    }

    fn update_zoom_in_action(&self) {
        let imp = self.imp();

        self.action_set_enabled("win.zoom-graph-in", imp.graph_view.can_zoom_in());
    }

    fn update_zoom_out_action(&self) {
        let imp = self.imp();

        self.action_set_enabled("win.zoom-graph-out", imp.graph_view.can_zoom_out());
    }

    fn update_reset_zoom_action(&self) {
        let imp = self.imp();

        self.action_set_enabled("win.reset-graph-zoom", imp.graph_view.can_reset_zoom());
    }
}

fn graphviz_file_filters() -> gio::ListStore {
    let filter = gtk::FileFilter::new();
    // Translators: DOT is an acronym, do not translate.
    filter.set_name(Some(&gettext("Graphviz DOT Files")));
    filter.add_mime_type("text/vnd.graphviz");

    let filters = gio::ListStore::new::<gtk::FileFilter>();
    filters.append(&filter);
    filters
}
