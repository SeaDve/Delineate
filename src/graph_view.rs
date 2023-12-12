use std::cell::RefCell;

use anyhow::{Context, Ok, Result};
use futures_channel::oneshot;
use gtk::{
    gio,
    glib::{self, clone, closure_local, translate::TryFromGlib},
    prelude::*,
    subclass::prelude::*,
};
use webkit::{javascriptcore::Value, prelude::*, ContextMenuAction};

use crate::config::GRAPHVIEWSRCDIR;

#[derive(Debug, Clone, Copy, glib::Enum)]
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
    use gtk::glib::{once_cell::sync::Lazy, subclass::Signal};

    use super::*;

    #[derive(Debug, glib::Properties)]
    #[properties(wrapper_type = super::GraphView)]
    pub struct GraphView {
        #[property(get)]
        pub(super) is_graph_loaded: Cell<bool>,

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
            settings.set_user_agent(Some(
                "Mozilla/5.0 (X11; Linux x86_64; rv:120.0) Gecko/20100101 Firefox/120.0",
            ));

            let context = webkit::WebContext::new();
            context.set_cache_model(webkit::CacheModel::DocumentViewer);

            Self {
                is_graph_loaded: Cell::new(false),
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

            let user_content_manager = self.view.user_content_manager().unwrap();

            user_content_manager.register_script_message_handler("graphError", None);
            user_content_manager.connect_script_message_received(
                Some("graphError"),
                clone!(@weak obj => move |_, value| {
                    let message = value.to_str();
                    obj.emit_by_name::<()>("graph-error", &[&message]);
                }),
            );

            user_content_manager.register_script_message_handler("graphLoaded", None);
            user_content_manager.connect_script_message_received(
                Some("graphLoaded"),
                clone!(@weak obj => move |_, _| {
                    obj.set_graph_loaded(true);
                    obj.emit_graph_loaded();
                }),
            );
        }

        fn signals() -> &'static [Signal] {
            static SIGNALS: Lazy<Vec<Signal>> = Lazy::new(|| {
                vec![
                    Signal::builder("graph-loaded").build(),
                    Signal::builder("graph-error")
                        .param_types([String::static_type()])
                        .build(),
                ]
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

    pub fn connect_graph_loaded<F>(&self, f: F) -> glib::SignalHandlerId
    where
        F: Fn(&Self) + 'static,
    {
        self.connect_closure(
            "graph-loaded",
            true,
            closure_local!(|obj: &Self| {
                f(obj);
            }),
        )
    }

    pub fn connect_graph_error<F>(&self, f: F) -> glib::SignalHandlerId
    where
        F: Fn(&Self, &str) + 'static,
    {
        self.connect_closure(
            "graph-error",
            true,
            closure_local!(|obj: &Self, message: &str| {
                f(obj, message);
            }),
        )
    }

    pub async fn render(&self, dot_src: &str, engine: Engine) -> Result<()> {
        self.set_graph_loaded(false);

        self.call_js_func("render", &[&dot_src, &engine.as_raw()])
            .await?;

        Ok(())
    }

    pub async fn get_svg(&self) -> Result<Option<glib::Bytes>> {
        let ret = self.call_js_func("getSvg", &[]).await?;

        if ret.is_null() {
            Ok(None)
        } else {
            let bytes = ret
                .to_string_as_bytes()
                .context("Failed to get ret as bytes")?;
            Ok(Some(bytes))
        }
    }

    async fn call_js_func(&self, func_name: &str, args: &[&dyn ToVariant]) -> Result<Value> {
        let imp = self.imp();

        self.ensure_index_loaded().await?;

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

    fn set_graph_loaded(&self, is_graph_loaded: bool) {
        if is_graph_loaded == self.is_graph_loaded() {
            return;
        }

        self.imp().is_graph_loaded.set(is_graph_loaded);
        self.notify_is_graph_loaded();
    }

    fn emit_graph_loaded(&self) {
        self.emit_by_name::<()>("graph-loaded", &[]);
    }

    async fn ensure_index_loaded(&self) -> Result<()> {
        let imp = self.imp();

        imp.index_loaded
            .get_or_try_init(|| async {
                let graph_view_src_dir = gio::File::for_path(GRAPHVIEWSRCDIR);

                let (index_bytes, _) = graph_view_src_dir
                    .child("index.html")
                    .load_bytes_future()
                    .await?;

                let (tx, rx) = oneshot::channel();
                let tx = RefCell::new(Some(tx));

                let handler_id = imp.view.connect_load_changed(
                    clone!(@weak imp => @default-panic, move |_, load_event| {
                        if load_event == webkit::LoadEvent::Finished {
                            if let Some(tx) = tx.take() {
                                tx.send(()).unwrap();
                            }
                        }
                    }),
                );

                // Needs to add trailing slash to base_uri
                let base_uri = format!("{}/", graph_view_src_dir.uri());
                imp.view
                    .load_bytes(&index_bytes, None, None, Some(&base_uri));

                rx.await.unwrap();
                imp.view.disconnect(handler_id);

                tracing::debug!("Loaded index.html from {}", graph_view_src_dir.uri());

                Ok(())
            })
            .await?;

        Ok(())
    }
}

impl Default for GraphView {
    fn default() -> Self {
        Self::new()
    }
}
