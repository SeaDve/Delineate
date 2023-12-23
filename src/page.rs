use std::time::Duration;

use adw::prelude::*;
use anyhow::{Context, Result};
use gettextrs::gettext;
use gtk::{
    gdk_pixbuf, gio,
    glib::{self, clone, closure, once_cell::sync::Lazy},
    subclass::prelude::*,
};
use gtk_source::prelude::*;
use regex::Regex;

use crate::{
    cancelled::Cancelled, document::Document, format::Format, graph_view::LayoutEngine,
    i18n::gettext_f, utils, window::Window,
};

const DRAW_GRAPH_PRIORITY: glib::Priority = glib::Priority::DEFAULT_IDLE;
const DRAW_GRAPH_INTERVAL: Duration = Duration::from_secs(1);

static SYNTAX_ERROR_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"syntax error in line (\d+)").expect("Failed to compile regex"));

mod imp {
    use std::{
        cell::{Cell, OnceCell, RefCell},
        marker::PhantomData,
    };

    use crate::{error_gutter_renderer::ErrorGutterRenderer, graph_view::GraphView};

    use super::*;

    #[derive(Default, glib::Properties, gtk::CompositeTemplate)]
    #[properties(wrapper_type = super::Page)]
    #[template(resource = "/io/github/seadve/Dagger/ui/page.ui")]
    pub struct Page {
        #[property(get = Self::title)]
        pub(super) title: PhantomData<String>,
        #[property(get = Self::is_busy)]
        pub(super) is_busy: PhantomData<bool>,
        #[property(get = Self::is_modified)]
        pub(super) is_modified: PhantomData<bool>,
        #[property(get = Self::can_save)]
        pub(super) can_save: PhantomData<bool>,
        #[property(get = Self::can_export)]
        pub(super) can_export: PhantomData<bool>,

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
        pub(super) layout_engine_drop_down: TemplateChild<gtk::DropDown>,
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
    impl ObjectSubclass for Page {
        const NAME: &'static str = "DaggerPage";
        type Type = super::Page;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();

            klass.install_action("page.go-to-error", None, |obj, _, _| {
                let imp = obj.imp();

                let line = imp.line_with_error.get().unwrap();
                let mut iter = imp.view.buffer().iter_at_line(line as i32).unwrap();
                imp.view.scroll_to_iter(&mut iter, 0.0, true, 0.0, 0.5);
            });

            klass.install_action_async("page.zoom-graph-in", None, |obj, _, _| async move {
                if let Err(err) = obj.imp().graph_view.zoom_in().await {
                    tracing::error!("Failed to zoom in: {:?}", err);
                }
            });

            klass.install_action_async("page.zoom-graph-out", None, |obj, _, _| async move {
                if let Err(err) = obj.imp().graph_view.zoom_out().await {
                    tracing::error!("Failed to zoom out: {:?}", err);
                }
            });

            klass.install_action_async("page.reset-graph-zoom", None, |obj, _, _| async move {
                if let Err(err) = obj.imp().graph_view.reset_zoom().await {
                    tracing::error!("Failed to reset zoom: {:?}", err);
                }
            });

            klass.install_action_async("page.show-in-files", Some("s"), |obj, _, arg| async move {
                let uri = arg.unwrap().get::<String>().unwrap();

                let file = gio::File::for_uri(&uri);
                let file_launcher = gtk::FileLauncher::new(Some(&file));
                if let Err(err) = file_launcher
                    .open_containing_folder_future(Some(&obj.window()))
                    .await
                {
                    tracing::error!("Failed to show in Files: {:?}", err);
                    obj.add_message_toast(&gettext("Failed to show in Files"));
                }
            });
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for Page {
        fn constructed(&self) {
            self.parent_constructed();

            let obj = self.obj();

            self.document_binding_group
                .bind("busy-progress", &*self.progress_bar, "fraction")
                .sync_create()
                .build();
            self.document_binding_group
                .bind("is-busy", &*self.progress_bar, "visible")
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
                Some("title"),
                clone!(@weak obj => move |_, _| {
                    obj.notify_title();
                }),
            );
            document_signal_group.connect_notify_local(
                Some("is-modified"),
                clone!(@weak obj => move |_, _| {
                    obj.notify_is_modified();
                }),
            );
            document_signal_group.connect_notify_local(
                Some("loading"),
                clone!(@weak obj => move |_, _| {
                    obj.notify_is_busy();
                    obj.notify_can_save();
                }),
            );
            document_signal_group.connect_notify_local(
                Some("is-busy"),
                clone!(@weak obj => move |_, _| {
                    obj.notify_is_busy();
                }),
            );
            self.document_signal_group
                .set(document_signal_group)
                .unwrap();

            self.layout_engine_drop_down
                .set_expression(Some(&gtk::ClosureExpression::new::<glib::GString>(
                    &[] as &[gtk::Expression],
                    closure!(|list_item: adw::EnumListItem| list_item.name()),
                )));
            self.layout_engine_drop_down
                .set_model(Some(&adw::EnumListModel::new(LayoutEngine::static_type())));
            self.layout_engine_drop_down
                .connect_selected_notify(clone!(@weak obj => move |_| {
                    obj.queue_draw_graph();
                }));

            let gutter = ViewExt::gutter(&*self.view, gtk::TextWindowType::Left);
            let was_inserted = gutter.insert(&self.error_gutter_renderer, 0);
            debug_assert!(was_inserted);

            self.go_to_error_revealer
                .connect_child_revealed_notify(clone!(@weak obj => move |_| {
                    obj.update_go_to_error_revealer_can_target();
                }));
            self.error_gutter_renderer
                .connect_has_visible_errors_notify(clone!(@weak obj => move |_| {
                    obj.update_go_to_error_revealer_reveal_child();
                }));

            self.graph_view
                .connect_is_graph_loaded_notify(clone!(@weak obj => move |_| {
                    obj.notify_can_export();
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
                DRAW_GRAPH_PRIORITY,
                clone!(@weak obj => async move {
                    obj.start_draw_graph_loop().await;
                }),
            );

            obj.set_document(&Document::new());

            obj.update_go_to_error_revealer_reveal_child();
            obj.update_go_to_error_revealer_can_target();
            obj.update_zoom_level_button();
            obj.update_zoom_in_action();
            obj.update_zoom_out_action();
            obj.update_reset_zoom_action();
        }

        fn dispose(&self) {
            self.dispose_template();
        }
    }

    impl WidgetImpl for Page {}

    impl Page {
        fn title(&self) -> String {
            let title = self.obj().document().title();
            if title.is_empty() {
                gettext("Untitled Document")
            } else {
                title
            }
        }

        fn is_busy(&self) -> bool {
            let document = self.obj().document();

            document.is_loading() || document.is_busy()
        }

        fn is_modified(&self) -> bool {
            self.obj().document().is_modified()
        }

        fn can_save(&self) -> bool {
            !self.obj().document().is_loading()
        }

        fn can_export(&self) -> bool {
            self.graph_view.is_graph_loaded()
        }
    }
}

glib::wrapper! {
    pub struct Page(ObjectSubclass<imp::Page>)
        @extends gtk::Widget;
}

impl Page {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub async fn load_file(&self, file: gio::File) -> Result<()> {
        let document = Document::for_file(file);
        self.set_document(&document);
        document.load().await?;
        Ok(())
    }

    pub async fn save_document(&self) -> Result<()> {
        let document = self.document();

        if document.is_draft() {
            let dialog = gtk::FileDialog::builder()
                .title(gettext("Save Document"))
                .filters(&utils::graphviz_file_filters())
                .modal(true)
                .initial_name(format!("{}.gv", document.title()))
                .build();
            let file = dialog.save_future(Some(&self.window())).await?;

            document.save_as(&file).await?;
        } else {
            document.save().await?;
        }

        Ok(())
    }

    pub async fn save_document_as(&self) -> Result<()> {
        let document = self.document();

        let dialog = gtk::FileDialog::builder()
            .title(gettext("Save Document As"))
            .filters(&utils::graphviz_file_filters())
            .modal(true)
            .initial_name(format!("{}.gv", document.title()))
            .build();
        let file = dialog.save_future(Some(&self.window())).await?;

        document.save_as(&file).await?;

        Ok(())
    }

    pub async fn export_graph(&self, format: Format) -> Result<()> {
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
        let file = dialog.save_future(Some(&self.window())).await?;

        let svg_bytes = imp.graph_view.get_svg().await?;

        let bytes = match format {
            Format::Svg => svg_bytes,
            Format::Png | Format::Jpeg => {
                // TODO improve resolution

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

        let toast = adw::Toast::builder()
            .title(gettext("Graph exported"))
            .button_label(gettext("Show in Files"))
            .action_name("page.show-in-files")
            .action_target(&file.uri().to_variant())
            .build();
        self.add_toast(toast);

        tracing::debug!(uri = %file.uri(), "Graph exported");

        Ok(())
    }

    pub fn document(&self) -> Document {
        self.imp().view.buffer().downcast().unwrap()
    }

    pub fn set_paned_position(&self, position: i32) {
        self.imp().paned.set_position(position);
    }

    pub fn paned_position(&self) -> i32 {
        self.imp().paned.position()
    }

    pub fn set_layout_engine(&self, engine: LayoutEngine) {
        let imp = self.imp();
        imp.layout_engine_drop_down.set_selected(engine as u32);
    }

    pub fn layout_engine(&self) -> LayoutEngine {
        let imp = self.imp();
        let selected_item = imp
            .layout_engine_drop_down
            .selected_item()
            .unwrap()
            .downcast::<adw::EnumListItem>()
            .unwrap();
        LayoutEngine::try_from(selected_item.value()).unwrap()
    }

    fn window(&self) -> Window {
        self.root().unwrap().downcast().unwrap()
    }

    fn add_toast(&self, toast: adw::Toast) {
        self.window().add_toast(toast);
    }

    fn add_message_toast(&self, message: &str) {
        self.window().add_message_toast(message);
    }

    fn set_document(&self, document: &Document) {
        let imp = self.imp();

        imp.view.set_buffer(Some(document));

        imp.document_binding_group.set_source(Some(document));

        let document_signal_group = imp.document_signal_group.get().unwrap();
        document_signal_group.set_target(Some(document));

        self.notify_title();
        self.notify_is_busy();
        self.notify_is_modified();
        self.notify_can_save();
    }

    fn queue_draw_graph(&self) {
        let imp = self.imp();

        imp.queued_draw_graph.set(true);

        // If we're not rendering a graph, skip the timeout.
        if !imp.graph_view.is_rendering() {
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
                glib::timeout_future_with_priority(DRAW_GRAPH_PRIORITY, DRAW_GRAPH_INTERVAL),
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
                .set_data(&self.document().contents(), self.layout_engine())
                .await
            {
                tracing::error!("Failed to render: {:?}", err);
            }
        }
    }

    fn handle_document_text_changed(&self) {
        let imp = self.imp();

        imp.error_gutter_renderer.clear_errors();

        imp.line_with_error.set(None);
        self.update_go_to_error_revealer_reveal_child();

        self.queue_draw_graph();
    }

    fn handle_graph_view_error(&self, message: &str) {
        let imp = self.imp();

        let message = message.trim();

        if let Some(captures) = SYNTAX_ERROR_REGEX.captures(message) {
            tracing::trace!("Syntax error: {}", message);

            let raw_line_number = captures[1].parse::<u32>().unwrap();
            // Subtract 1 since line numbers from the error starts at 1.
            let line_number = raw_line_number - 1;
            imp.error_gutter_renderer.set_error(line_number, message);

            imp.line_with_error.set(Some(line_number));
            self.update_go_to_error_revealer_reveal_child();
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
            .transient_for(&self.window())
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

        let save_response_text = if document.is_draft() {
            gettext("Save As…")
        } else {
            gettext("Save")
        };
        dialog.add_response(SAVE_RESPONSE_ID, &save_response_text);
        dialog.set_response_appearance(SAVE_RESPONSE_ID, adw::ResponseAppearance::Suggested);

        match dialog.choose_future().await.as_str() {
            CANCEL_RESPONSE_ID => Err(Cancelled.into()),
            DISCARD_RESPONSE_ID => Ok(()),
            SAVE_RESPONSE_ID => self.save_document().await,
            _ => unreachable!(),
        }
    }

    fn update_go_to_error_revealer_reveal_child(&self) {
        let imp = self.imp();

        imp.go_to_error_revealer.set_reveal_child(
            imp.line_with_error.get().is_some() && !imp.error_gutter_renderer.has_visible_errors(),
        );
    }

    fn update_go_to_error_revealer_can_target(&self) {
        let imp = self.imp();

        imp.go_to_error_revealer
            .set_can_target(imp.go_to_error_revealer.is_child_revealed());
    }

    fn update_zoom_level_button(&self) {
        let imp = self.imp();

        let zoom_level = imp.graph_view.zoom_level();
        imp.zoom_level_button
            .set_label(&format!("{:.0}%", zoom_level * 100.0));
    }

    fn update_zoom_in_action(&self) {
        let imp = self.imp();

        self.action_set_enabled("page.zoom-graph-in", imp.graph_view.can_zoom_in());
    }

    fn update_zoom_out_action(&self) {
        let imp = self.imp();

        self.action_set_enabled("page.zoom-graph-out", imp.graph_view.can_zoom_out());
    }

    fn update_reset_zoom_action(&self) {
        let imp = self.imp();

        self.action_set_enabled("page.reset-graph-zoom", imp.graph_view.can_reset_zoom());
    }
}

impl Default for Page {
    fn default() -> Self {
        Self::new()
    }
}
