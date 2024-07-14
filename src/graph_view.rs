use std::cell::RefCell;

use anyhow::{ensure, Context, Result};
use futures_channel::oneshot;
use gtk::{
    gio,
    glib::{self, clone, closure_local, translate::TryFromGlib},
    prelude::*,
    subclass::prelude::*,
};
use serde::{Deserialize, Serialize};
use webkit::{javascriptcore::Value, prelude::*, ContextMenuAction};

use crate::{config::GRAPHVIEWSRCDIR, utils};

const INIT_END_MESSAGE_ID: &str = "initEnd";
const ERROR_MESSAGE_ID: &str = "error";
const IS_GRAPH_LOADED_CHANGED_MESSAGE_ID: &str = "isGraphLoadedChanged";
const IS_RENDERING_CHANGED_MESSAGE_ID: &str = "isRenderingChanged";
const ZOOM_LEVEL_CHANGED_MESSAGE_ID: &str = "zoomLevelChanged";

const ZOOM_FACTOR: f64 = 1.5;
const MIN_ZOOM_LEVEL: f64 = 0.1;
const MAX_ZOOM_LEVEL: f64 = 100.0;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, glib::Enum)]
#[repr(i32)]
#[enum_type(name = "DelineateGraphViewEngine")]
pub enum LayoutEngine {
    Dot,
    Circo,
    Fdp,
    Sfdp,
    Neato,
    Osage,
    Patchwork,
    Twopi,
}

impl TryFrom<i32> for LayoutEngine {
    type Error = i32;

    fn try_from(val: i32) -> Result<Self, Self::Error> {
        unsafe { Self::try_from_glib(val) }
    }
}

impl LayoutEngine {
    fn as_raw(&self) -> &'static str {
        match self {
            Self::Dot => "dot",
            Self::Circo => "circo",
            Self::Fdp => "fdp",
            Self::Sfdp => "sfdp",
            Self::Neato => "neato",
            Self::Osage => "osage",
            Self::Patchwork => "patchwork",
            Self::Twopi => "twopi",
        }
    }
}

mod imp {
    use std::{cell::Cell, marker::PhantomData};

    use async_lock::OnceCell;
    use glib::subclass::Signal;
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, glib::Properties)]
    #[properties(wrapper_type = super::GraphView)]
    pub struct GraphView {
        #[property(get)]
        pub(super) is_graph_loaded: Cell<bool>,
        #[property(get)]
        pub(super) is_rendering: Cell<bool>,
        #[property(get)]
        pub(super) zoom_level: Cell<f64>,
        #[property(get = Self::can_zoom_in)]
        pub(super) can_zoom_in: PhantomData<bool>,
        #[property(get = Self::can_zoom_out)]
        pub(super) can_zoom_out: PhantomData<bool>,
        #[property(get = Self::can_reset_zoom)]
        pub(super) can_reset_zoom: PhantomData<bool>,

        pub(super) view: webkit::WebView,
        pub(super) index_loaded: OnceCell<()>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for GraphView {
        const NAME: &'static str = "DelineateGraphView";
        type Type = super::GraphView;
        type ParentType = gtk::Widget;

        fn new() -> Self {
            let settings = webkit::Settings::new();

            if utils::is_devel_profile() {
                settings.set_enable_developer_extras(true);
                settings.set_enable_write_console_messages_to_stdout(true);
            }

            let context = webkit::WebContext::new();
            context.set_cache_model(webkit::CacheModel::DocumentViewer);

            Self {
                is_graph_loaded: Cell::new(false),
                is_rendering: Cell::new(false),
                zoom_level: Cell::new(1.0),
                can_zoom_in: PhantomData,
                can_zoom_out: PhantomData,
                can_reset_zoom: PhantomData,
                view: glib::Object::builder()
                    .property("visible", false)
                    .property("settings", settings)
                    .property("web-context", context)
                    .build(),
                index_loaded: OnceCell::new(),
            }
        }

        fn class_init(klass: &mut Self::Class) {
            klass.set_layout_manager_type::<gtk::BinLayout>();
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for GraphView {
        fn constructed(&self) {
            self.parent_constructed();

            let obj = self.obj();

            self.view.set_parent(&*obj);

            self.view.connect_web_process_terminated(|_, reason| {
                tracing::error!("Web process terminated: {:?}", reason);
            });
            self.view.connect_is_web_process_responsive_notify(|view| {
                if !view.is_web_process_responsive() {
                    tracing::warn!("Web process is unresponsive");
                }
            });
            self.view.connect_context_menu(move |_, ctx_menu, _| {
                for item in ctx_menu.items() {
                    if !matches!(item.stock_action(), ContextMenuAction::InspectElement) {
                        ctx_menu.remove(&item);
                    }
                }

                if ctx_menu.n_items() == 0 {
                    return true;
                }

                false
            });

            obj.connect_script_message_received(
                ERROR_MESSAGE_ID,
                clone!(
                    #[weak]
                    obj,
                    move |_, value| {
                        let message = value.to_str();
                        obj.emit_by_name::<()>("error", &[&message]);
                    }
                ),
            );
            obj.connect_script_message_received(
                IS_GRAPH_LOADED_CHANGED_MESSAGE_ID,
                clone!(
                    #[weak]
                    obj,
                    move |_, value| {
                        let is_graph_loaded = value.to_boolean();
                        obj.set_graph_loaded(is_graph_loaded);
                    }
                ),
            );
            obj.connect_script_message_received(
                IS_RENDERING_CHANGED_MESSAGE_ID,
                clone!(
                    #[weak]
                    obj,
                    move |_, value| {
                        let is_rendering = value.to_boolean();
                        obj.set_rendering(is_rendering);
                    }
                ),
            );
            obj.connect_script_message_received(
                ZOOM_LEVEL_CHANGED_MESSAGE_ID,
                clone!(
                    #[weak]
                    obj,
                    move |_, value| {
                        let zoom_level = value.to_double();
                        obj.set_zoom_level(zoom_level);
                    }
                ),
            );

            utils::spawn(clone!(
                #[weak]
                obj,
                async move {
                    if let Err(err) = obj.ensure_view_initialized().await {
                        tracing::error!("Failed to initialize view: {:?}", err);
                    }
                }
            ));
        }

        fn dispose(&self) {
            self.view.unparent();
        }

        fn signals() -> &'static [Signal] {
            static SIGNALS: Lazy<Vec<Signal>> = Lazy::new(|| {
                vec![Signal::builder("error")
                    .param_types([String::static_type()])
                    .build()]
            });

            SIGNALS.as_ref()
        }
    }

    impl WidgetImpl for GraphView {}

    impl GraphView {
        fn can_zoom_in(&self) -> bool {
            let obj = self.obj();

            obj.zoom_level() < MAX_ZOOM_LEVEL && obj.is_graph_loaded()
        }

        fn can_zoom_out(&self) -> bool {
            let obj = self.obj();

            obj.zoom_level() > MIN_ZOOM_LEVEL && obj.is_graph_loaded()
        }

        fn can_reset_zoom(&self) -> bool {
            let obj = self.obj();

            // FIXME Also only allow it when not on default zoom level & position
            obj.is_graph_loaded()
        }
    }
}

glib::wrapper! {
    pub struct GraphView(ObjectSubclass<imp::GraphView>)
        @extends gtk::Widget;
}

impl GraphView {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn connect_error<F>(&self, f: F) -> glib::SignalHandlerId
    where
        F: Fn(&Self, &str) + 'static,
    {
        self.connect_closure(
            "error",
            false,
            closure_local!(|obj: &Self, message: &str| {
                f(obj, message);
            }),
        )
    }

    pub async fn set_data(&self, dot_src: &str, layout_engine: LayoutEngine) -> Result<()> {
        self.call_js_method("setData", &[&dot_src, &layout_engine.as_raw()])
            .await?;
        Ok(())
    }

    pub async fn zoom_in(&self) -> Result<()> {
        self.set_zoom_level_by(ZOOM_FACTOR).await?;
        Ok(())
    }

    pub async fn zoom_out(&self) -> Result<()> {
        self.set_zoom_level_by(ZOOM_FACTOR.recip()).await?;
        Ok(())
    }

    pub async fn reset_zoom(&self) -> Result<()> {
        self.call_js_method("resetZoom", &[]).await?;
        Ok(())
    }

    pub async fn get_svg(&self) -> Result<glib::Bytes> {
        let value = self.call_js_method("getSvgString", &[]).await?;

        ensure!(!value.is_null(), "SVG is null");

        let bytes = value
            .to_string_as_bytes()
            .context("Failed to get value as bytes")?;
        Ok(bytes)
    }

    async fn set_zoom_level_by(&self, factor: f64) -> Result<()> {
        self.call_js_method("setZoomLevelBy", &[&factor]).await?;
        Ok(())
    }

    async fn call_js_method(&self, method_name: &str, args: &[&dyn ToVariant]) -> Result<Value> {
        self.ensure_view_initialized().await?;
        self.call_js_method_inner(method_name, args).await
    }

    async fn call_js_method_inner(
        &self,
        method_name: &str,
        args: &[&dyn ToVariant],
    ) -> Result<Value> {
        let imp = self.imp();

        let args = args
            .iter()
            .enumerate()
            .map(|(index, value)| (format!("arg{}", index), value.to_variant()))
            .collect::<Vec<_>>();

        let body = format!(
            "return graphView.{}({})",
            method_name,
            args.iter()
                .map(|(name, _)| name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );

        let args = if args.is_empty() {
            None
        } else {
            let arg_dict = glib::VariantDict::new(None);
            for (name, value) in args {
                arg_dict.insert(&name, value);
            }
            Some(arg_dict.end())
        };

        let value = imp
            .view
            .call_async_javascript_function_future(&body, args.as_ref(), None, None)
            .await
            .with_context(|| format!("Failed to call `{}`", method_name))?;
        tracing::trace!(value = %value.to_str(), "JS method returned");

        Ok(value)
    }

    fn connect_script_message_received<F>(&self, message_id: &str, f: F) -> glib::SignalHandlerId
    where
        F: Fn(&webkit::UserContentManager, &Value) + 'static,
    {
        let imp = self.imp();

        let user_content_manager = imp.view.user_content_manager().unwrap();

        let was_successful = user_content_manager.register_script_message_handler(message_id, None);
        debug_assert!(was_successful);

        user_content_manager.connect_script_message_received(Some(message_id), f)
    }

    fn set_graph_loaded(&self, is_graph_loaded: bool) {
        if is_graph_loaded == self.is_graph_loaded() {
            return;
        }

        self.imp().is_graph_loaded.set(is_graph_loaded);
        self.notify_can_zoom_in();
        self.notify_can_zoom_out();
        self.notify_can_reset_zoom();
        self.notify_is_graph_loaded();
    }

    fn set_rendering(&self, is_rendering: bool) {
        if is_rendering == self.is_rendering() {
            return;
        }

        self.imp().is_rendering.set(is_rendering);
        self.notify_is_rendering();
    }

    fn set_zoom_level(&self, zoom_level: f64) {
        if zoom_level == self.zoom_level() {
            return;
        }

        self.imp().zoom_level.set(zoom_level);
        self.notify_can_zoom_in();
        self.notify_can_zoom_out();
        self.notify_zoom_level();
    }

    async fn ensure_view_initialized(&self) -> Result<()> {
        let imp = self.imp();

        // FIXME Use a proper async Once
        imp.index_loaded
            .get_or_try_init(|| self.init_view())
            .await?;

        Ok(())
    }

    async fn init_view(&self) -> Result<()> {
        let imp = self.imp();

        let graph_view_src_dir = gio::File::for_path(GRAPHVIEWSRCDIR);
        let index_file = graph_view_src_dir.child("index.html");

        let (index_bytes, _) = index_file.load_bytes_future().await?;

        let (load_tx, load_rx) = oneshot::channel();
        let load_tx = RefCell::new(Some(load_tx));

        let load_handler_id = imp.view.connect_load_changed(move |_, load_event| {
            if load_event == webkit::LoadEvent::Finished {
                if let Some(tx) = load_tx.take() {
                    tx.send(()).unwrap();
                }
            }
        });

        let (init_tx, init_rx) = oneshot::channel();
        let init_tx = RefCell::new(Some(init_tx));

        let init_handler_id =
            self.connect_script_message_received(INIT_END_MESSAGE_ID, move |_, _| {
                if let Some(tx) = init_tx.take() {
                    tx.send(()).unwrap();
                }
            });

        // Needs to add trailing slash to base_uri
        let base_uri = format!("{}/", graph_view_src_dir.uri());
        imp.view
            .load_bytes(&index_bytes, None, None, Some(&base_uri));

        load_rx.await.unwrap();
        imp.view.disconnect(load_handler_id);

        tracing::debug!("Loaded index.html from {}", index_file.uri());

        init_rx.await.unwrap();
        let user_content_manager = imp.view.user_content_manager().unwrap();
        user_content_manager.unregister_script_message_handler(INIT_END_MESSAGE_ID, None);
        user_content_manager.disconnect(init_handler_id);

        self.call_js_method_inner("setZoomScaleExtent", &[&MIN_ZOOM_LEVEL, &MAX_ZOOM_LEVEL])
            .await
            .context("Failed to set zoom scale extent")?;

        let version = self
            .call_js_method_inner("graphvizVersion", &[])
            .await
            .context("Failed to get version")?
            .to_str();
        tracing::debug!(%version, "Initialized Graphviz");

        // Hide view while it's loading to prevent flickering from the delayed
        // style sheet loading.
        imp.view.set_visible(true);

        Ok(())
    }
}
