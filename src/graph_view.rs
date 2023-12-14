use std::cell::RefCell;

use anyhow::{Context, Result};
use futures_channel::oneshot;
use gtk::{
    gio,
    glib::{self, clone, closure_local, translate::TryFromGlib},
    prelude::*,
    subclass::prelude::*,
};
use webkit::{javascriptcore::Value, prelude::*, ContextMenuAction};

use crate::{config::GRAPHVIEWSRCDIR, utils};

const INIT_END_MESSAGE_ID: &str = "initEnd";
const ERROR_MESSAGE_ID: &str = "error";
const IS_GRAPH_LOADED_CHANGED_MESSAGE_ID: &str = "isGraphLoadedChanged";
const IS_RENDERING_CHANGED_MESSAGE_ID: &str = "isRenderingChanged";
const ZOOM_LEVEL_CHANGED_MESSAGE_ID: &str = "zoomLevelChanged";

const ZOOM_FACTOR: f64 = 1.5;
const MIN_ZOOM_LEVEL: f64 = 0.05;
const MAX_ZOOM_LEVEL: f64 = 20.0;

#[derive(Debug, Clone, Copy, glib::Variant, glib::Enum)]
#[repr(i32)]
#[enum_type(name = "DaggerGraphViewEngine")]
pub enum Engine {
    Dot,
    Circo,
    Fdp,
    Sfdp,
    Neato,
    Osage,
    Patchwork,
    Twopi,
}

impl TryFrom<i32> for Engine {
    type Error = i32;

    fn try_from(val: i32) -> Result<Self, Self::Error> {
        unsafe { Self::try_from_glib(val) }
    }
}

impl Engine {
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
    use std::cell::Cell;

    use async_lock::OnceCell;
    use glib::{once_cell::sync::Lazy, subclass::Signal};

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
        #[property(get)]
        pub(super) can_zoom_in: Cell<bool>,
        #[property(get)]
        pub(super) can_zoom_out: Cell<bool>,
        #[property(get)]
        pub(super) can_reset_zoom: Cell<bool>,

        pub(super) view: webkit::WebView,
        pub(super) index_loaded: OnceCell<()>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for GraphView {
        const NAME: &'static str = "DaggerGraphView";
        type Type = super::GraphView;
        type ParentType = gtk::Widget;

        fn new() -> Self {
            let settings = webkit::Settings::new();
            settings.set_enable_developer_extras(true);
            settings.set_enable_write_console_messages_to_stdout(true);

            let context = webkit::WebContext::new();
            context.set_cache_model(webkit::CacheModel::DocumentViewer);

            Self {
                is_graph_loaded: Cell::new(false),
                is_rendering: Cell::new(false),
                zoom_level: Cell::new(1.0),
                can_zoom_in: Cell::new(false),
                can_zoom_out: Cell::new(false),
                can_reset_zoom: Cell::new(false),
                view: glib::Object::builder()
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
            self.view.connect_context_menu(
                clone!(@weak obj => @default-panic, move |_, ctx_menu, _| {
                    for item in ctx_menu.items() {
                        if !matches!(item.stock_action(), ContextMenuAction::InspectElement) {
                            ctx_menu.remove(&item);
                        }
                    }

                    if ctx_menu.n_items() == 0 {
                        return true;
                    }

                    false
                }),
            );

            obj.connect_script_message_received(
                ERROR_MESSAGE_ID,
                clone!(@weak obj => move |_, value| {
                    let message = value.to_str();
                    obj.emit_by_name::<()>("error", &[&message]);
                }),
            );
            obj.connect_script_message_received(
                IS_GRAPH_LOADED_CHANGED_MESSAGE_ID,
                clone!(@weak obj => move |_, value| {
                    let is_graph_loaded = value.to_boolean();
                    obj.set_graph_loaded(is_graph_loaded);
                }),
            );
            obj.connect_script_message_received(
                IS_RENDERING_CHANGED_MESSAGE_ID,
                clone!(@weak obj => move |_, value| {
                    let is_rendering = value.to_boolean();
                    obj.imp().is_rendering.set(is_rendering);
                    obj.notify_is_rendering();
                }),
            );
            obj.connect_script_message_received(
                ZOOM_LEVEL_CHANGED_MESSAGE_ID,
                clone!(@weak obj => move |_, value| {
                    let zoom_level = value.to_double();
                    obj.set_zoom_level(zoom_level);
                }),
            );

            utils::spawn(
                glib::Priority::default(),
                clone!(@weak obj => async move {
                    if let Err(err) = obj.ensure_view_initialized().await {
                        tracing::error!("Failed to initialize view: {:?}", err);
                    }
                }),
            );
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
            true,
            closure_local!(|obj: &Self, message: &str| {
                f(obj, message);
            }),
        )
    }

    pub async fn set_data(&self, dot_src: &str, engine: Engine) -> Result<()> {
        self.set_graph_loaded(false);

        self.call_js_func("graphView.setData", &[&dot_src, &engine.as_raw()])
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
        self.call_js_func("graphView.resetZoom", &[]).await?;
        Ok(())
    }

    pub async fn get_svg(&self) -> Result<Option<glib::Bytes>> {
        let ret = self.call_js_func("graphView.getSvgString", &[]).await?;

        if ret.is_null() {
            return Ok(None);
        }

        let bytes = ret
            .to_string_as_bytes()
            .context("Failed to get ret as bytes")?;
        Ok(Some(bytes))
    }

    async fn set_zoom_level_by(&self, factor: f64) -> Result<()> {
        self.call_js_func("graphView.setZoomLevelBy", &[&factor])
            .await?;
        Ok(())
    }

    async fn call_js_func(&self, func_name: &str, args: &[&dyn ToVariant]) -> Result<Value> {
        self.ensure_view_initialized().await?;
        self.call_js_func_inner(func_name, args).await
    }

    async fn call_js_func_inner(&self, func_name: &str, args: &[&dyn ToVariant]) -> Result<Value> {
        let imp = self.imp();

        let args = args
            .iter()
            .enumerate()
            .map(|(index, value)| (format!("arg{}", index), value))
            .collect::<Vec<_>>();

        let body = format!(
            "return {}({})",
            func_name,
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
                arg_dict.insert(&name, value.to_variant());
            }
            Some(arg_dict.end())
        };

        let ret_value = imp
            .view
            .call_async_javascript_function_future(&body, args.as_ref(), None, None)
            .await
            .with_context(|| format!("Failed to call `{}`", func_name))?;
        tracing::trace!(ret = %ret_value.to_str(), "JS function returned");

        Ok(ret_value)
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
        self.update_can_zoom();
        self.notify_is_graph_loaded();
    }

    fn set_zoom_level(&self, zoom_level: f64) {
        if zoom_level == self.zoom_level() {
            return;
        }

        self.imp().zoom_level.set(zoom_level);
        self.update_can_zoom();
        self.notify_zoom_level();
    }

    async fn ensure_view_initialized(&self) -> Result<()> {
        let imp = self.imp();

        imp.index_loaded
            .get_or_try_init(|| async {
                let graph_view_src_dir = gio::File::for_path(GRAPHVIEWSRCDIR);

                let (index_bytes, _) = graph_view_src_dir
                    .child("index.html")
                    .load_bytes_future()
                    .await?;

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

                tracing::debug!("Loaded index.html from {}", graph_view_src_dir.uri());

                init_rx.await.unwrap();
                let user_content_manager = imp.view.user_content_manager().unwrap();
                user_content_manager.unregister_script_message_handler(INIT_END_MESSAGE_ID, None);
                user_content_manager.disconnect(init_handler_id);

                self.call_js_func_inner(
                    "graphView.setZoomScaleExtent",
                    &[&MIN_ZOOM_LEVEL, &MAX_ZOOM_LEVEL],
                )
                .await
                .context("Failed to set zoom scale extent")?;

                let version = self
                    .call_js_func_inner("graphView.graphvizVersion", &[])
                    .await
                    .context("Failed to get version")?
                    .to_str();
                tracing::debug!(%version, "Initialized Graphviz");

                anyhow::Ok(())
            })
            .await?;

        Ok(())
    }

    fn update_can_zoom(&self) {
        let imp = self.imp();

        let is_graph_loaded = self.is_graph_loaded();
        let zoom_level = self.zoom_level();

        imp.can_zoom_in
            .set(zoom_level < MAX_ZOOM_LEVEL && is_graph_loaded);
        imp.can_zoom_out
            .set(zoom_level > MIN_ZOOM_LEVEL && is_graph_loaded);
        // FIXME Also only allow it when not on default zoom level & position
        imp.can_reset_zoom.set(is_graph_loaded);

        self.notify_can_zoom_in();
        self.notify_can_zoom_out();
        self.notify_can_reset_zoom();
    }
}

impl Default for GraphView {
    fn default() -> Self {
        Self::new()
    }
}
