use std::{error, fmt, time::Duration};

use adw::{prelude::*, subclass::prelude::*};
use anyhow::{Context, Result};
use gettextrs::gettext;
use gtk::{
    gdk, gio,
    glib::{self, clone, closure},
};
use gtk_source::prelude::*;

use crate::{
    application::Application,
    config::PROFILE,
    document::Document,
    drag_overlay::DragOverlay,
    graphviz::{self, Format, Layout},
    i18n::gettext_f,
    utils,
};

// TODO
// * Find and replace
// * Better viewer, with bird's eye view
// * Tabs and/or multiple windows
// * Recent files
// * dot language server, with error handling on text view
// * modified file on disk handling

const DRAW_GRAPH_INTERVAL: Duration = Duration::from_secs(1);

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
        pub(super) view: TemplateChild<gtk_source::View>,
        #[template_child]
        pub(super) stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub(super) picture_page: TemplateChild<gtk::ScrolledWindow>,
        #[template_child]
        pub(super) picture: TemplateChild<gtk::Picture>,
        #[template_child]
        pub(super) error_page: TemplateChild<adw::StatusPage>,
        #[template_child]
        pub(super) layout_drop_down: TemplateChild<gtk::DropDown>,
        #[template_child]
        pub(super) spinner_revealer: TemplateChild<gtk::Revealer>,

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
                    "webp" => Format::Webp,
                    "pdf" => Format::Pdf,
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
                "changed",
                false,
                clone!(@weak obj => @default-panic, move |_| {
                    obj.queue_draw_graph();
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

            self.layout_drop_down.set_expression(Some(
                &gtk::ClosureExpression::new::<glib::GString>(
                    &[] as &[gtk::Expression],
                    closure!(|list_item: adw::EnumListItem| list_item.name()),
                ),
            ));
            self.layout_drop_down
                .set_model(Some(&adw::EnumListModel::new(Layout::static_type())));
            self.layout_drop_down
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

            utils::spawn(
                glib::Priority::DEFAULT_IDLE,
                clone!(@weak obj => async move {
                    obj.start_draw_graph_loop().await;
                }),
            );

            obj.set_document(&Document::draft());
            obj.update_export_graph_action();

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
                glib::spawn_future_local(clone!(@weak obj => async move {
                    if obj.handle_unsaved_changes(&prev_document).await.is_err() {
                        return;
                    }
                    obj.destroy();
                }));
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

        self.queue_draw_graph();
    }

    fn document(&self) -> Document {
        self.imp().view.buffer().downcast().unwrap()
    }

    fn selected_layout(&self) -> Layout {
        let imp = self.imp();
        let selected_item = imp
            .layout_drop_down
            .selected_item()
            .unwrap()
            .downcast::<adw::EnumListItem>()
            .unwrap();
        Layout::try_from(selected_item.value()).unwrap()
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

        graphviz::export(
            document.contents().as_bytes(),
            self.selected_layout(),
            format,
            &file.path().context("File has no path")?,
        )
        .await?;

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
        self.update_export_graph_action();
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

            match self.draw_graph().await {
                Ok(texture) => {
                    imp.picture.set_paintable(texture.as_ref());
                    imp.stack.set_visible_child(&*imp.picture_page);
                    tracing::debug!("Graph updated");
                }
                Err(err) => {
                    imp.picture.set_paintable(gdk::Paintable::NONE);
                    imp.stack.set_visible_child(&*imp.error_page);
                    imp.error_page
                        .set_description(Some(&glib::markup_escape_text(
                            err.to_string().trim_start_matches("Error: <stdin>:").trim(),
                        )));
                    tracing::error!("Failed to draw graph: {:?}", err);
                }
            }

            imp.spinner_revealer.set_reveal_child(false);
            self.update_export_graph_action();
        }
    }

    async fn draw_graph(&self) -> Result<Option<gdk::Texture>> {
        let contents = self.document().contents();

        if contents.is_empty() {
            return Ok(None);
        }

        let png_bytes =
            graphviz::generate(contents.as_bytes(), self.selected_layout(), Format::Svg).await?;

        let texture =
            gio::spawn_blocking(|| gdk::Texture::from_bytes(&glib::Bytes::from_owned(png_bytes)))
                .await
                .unwrap()?;

        Ok(Some(texture))
    }

    fn update_save_action(&self) {
        let is_loading = self.document().is_loading();
        self.action_set_enabled("win.save-document", !is_loading);
        self.action_set_enabled("win.save-document-as", !is_loading);
    }

    fn update_export_graph_action(&self) {
        let imp = self.imp();

        self.action_set_enabled(
            "win.export-graph",
            !imp.spinner_revealer.reveals_child() && imp.picture.paintable().is_some(),
        );
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
