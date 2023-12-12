use anyhow::Result;
use gtk::{
    gio,
    glib::{self, clone, closure_local, translate::TryFromGlib},
    prelude::*,
    subclass::prelude::*,
};
use webkit::prelude::*;

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
    use gtk::glib::{once_cell::sync::Lazy, subclass::Signal};

    use super::*;

    #[derive(Debug)]
    pub struct GraphView {
        pub(super) view: webkit::WebView,
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
                view: glib::Object::builder()
                    .property("settings", settings)
                    .property("web-context", context)
                    .build(),
            }
        }

        fn class_init(klass: &mut Self::Class) {
            klass.set_layout_manager_type::<gtk::BinLayout>();
        }
    }

    impl ObjectImpl for GraphView {
        fn constructed(&self) {
            self.parent_constructed();

            let obj = self.obj();

            self.view.set_parent(&*obj);

            self.view
                .connect_is_web_process_responsive_notify(clone!(@weak obj => move |view| {
                    if !view.is_web_process_responsive() {
                        tracing::warn!("Web process is unresponsive");
                    }
                }));

            // FIXME don't hardcode
            self.view.load_bytes(
                &gio::File::for_path("/app/src/graph_view/index.html")
                    .load_bytes(gio::Cancellable::NONE)
                    .unwrap()
                    .0,
                None,
                None,
                Some("file:///app/src/graph_view/"),
            );

            let user_content_manager = self.view.user_content_manager().unwrap();

            user_content_manager.register_script_message_handler("graphError", None);
            user_content_manager.connect_script_message_received(
                Some("graphError"),
                clone!(@weak obj => move |_, value| {
                    let message = value.to_str();
                    obj.emit_by_name::<()>("error", &[&message]);
                }),
            );

            user_content_manager.register_script_message_handler("graphLoaded", None);
            user_content_manager.connect_script_message_received(
                Some("graphLoaded"),
                clone!(@weak obj => move |_, _| {
                    obj.emit_loaded();
                }),
            );

            self.view.inspector().unwrap().show();
        }

        fn signals() -> &'static [Signal] {
            static SIGNALS: Lazy<Vec<Signal>> = Lazy::new(|| {
                vec![
                    Signal::builder("loaded").build(),
                    Signal::builder("error")
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

    pub fn connect_loaded<F>(&self, f: F) -> glib::SignalHandlerId
    where
        F: Fn(&Self) + 'static,
    {
        self.connect_closure(
            "loaded",
            true,
            closure_local!(|obj: &Self| {
                f(obj);
            }),
        )
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

    pub async fn render(&self, dot_src: &str, engine: Engine) -> Result<()> {
        let imp = self.imp();

        let dict = glib::VariantDict::new(None);
        dict.insert("dotSrc", &dot_src.to_variant());
        dict.insert("engine", &engine.as_raw().to_variant());
        let args = dict.end();

        let ret = imp
            .view
            .call_async_javascript_function_future(
                "render(dotSrc, engine)",
                Some(&args),
                None,
                None,
            )
            .await?;
        tracing::debug!(ret = %ret.to_str(), "Rendered");

        Ok(())
    }

    fn emit_loaded(&self) {
        self.emit_by_name::<()>("loaded", &[]);
    }
}

impl Default for GraphView {
    fn default() -> Self {
        Self::new()
    }
}
