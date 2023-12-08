use std::time::Duration;

use adw::{prelude::*, subclass::prelude::*};
use anyhow::Result;
use gettextrs::gettext;
use gtk::{
    gdk, gio,
    glib::{self, clone, closure},
};
use gtk_source::prelude::*;

use crate::{
    application::Application,
    config::{APP_ID, PROFILE},
    graphviz::{self, Format, Layout},
    utils,
};

const DRAW_GRAPH_INTERVAL: Duration = Duration::from_millis(100);

mod imp {
    use std::cell::Cell;

    use super::*;

    #[derive(Debug, Default, gtk::CompositeTemplate)]
    #[template(resource = "/io/github/seadve/Dagger/ui/window.ui")]
    pub struct Window {
        #[template_child]
        pub(super) buffer: TemplateChild<gtk_source::Buffer>,
        #[template_child]
        pub(super) picture: TemplateChild<gtk::Picture>,
        #[template_child]
        pub(super) layout_drop_down: TemplateChild<gtk::DropDown>,
        #[template_child]
        pub(super) spinner_revealer: TemplateChild<gtk::Revealer>,

        pub(super) queued_draw_graph: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Window {
        const NAME: &'static str = "DaggerWindow";
        type Type = super::Window;
        type ParentType = adw::ApplicationWindow;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();

            klass.install_action_async("win.open-file", None, |obj, _, _| async move {
                if let Err(err) = obj.open_file().await {
                    tracing::error!("Failed to open file: {:?}", err);
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

            let style_manager = adw::StyleManager::default();
            style_manager.connect_dark_notify(clone!(@weak obj => move |_| {
                obj.update_buffer_style_scheme();
            }));

            let language_manager = gtk_source::LanguageManager::default();
            if let Some(language) = language_manager.language("dot") {
                self.buffer.set_language(Some(&language));
                self.buffer.set_highlight_syntax(true);
            }

            self.buffer.connect_changed(clone!(@weak obj => move |_| {
                obj.queue_draw_graph();
            }));

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

            utils::spawn(
                glib::Priority::DEFAULT_IDLE,
                clone!(@weak obj => async move {
                    obj.start_draw_graph_loop().await;
                }),
            );

            obj.update_buffer_style_scheme();

            obj.load_window_size();
        }

        fn dispose(&self) {
            self.dispose_template();
        }
    }

    impl WidgetImpl for Window {}
    impl WindowImpl for Window {
        fn close_request(&self) -> glib::Propagation {
            if let Err(err) = self.obj().save_window_size() {
                tracing::warn!("Failed to save window state, {}", &err);
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

    async fn open_file(&self) -> Result<()> {
        let imp = self.imp();

        let filter = gtk::FileFilter::new();
        // Translators: DOT is a type of file, do not translate.
        filter.set_property("name", gettext("DOT Files"));
        filter.add_mime_type("text/vnd.graphviz");

        let filters = gio::ListStore::new::<gtk::FileFilter>();
        filters.append(&filter);

        let dialog = gtk::FileDialog::builder()
            .title(gettext("Open Circuit"))
            .filters(&filters)
            .modal(true)
            .build();
        let file = dialog.open_future(Some(self)).await?;

        let source_file = gtk_source::File::new();
        source_file.set_location(Some(&file));

        let (res, _) = gtk_source::FileLoader::new(&*imp.buffer, &source_file)
            .load_future(glib::Priority::default());
        res.await?;

        self.queue_draw_graph();

        Ok(())
    }

    fn save_window_size(&self) -> Result<(), glib::BoolError> {
        let settings = gio::Settings::new(APP_ID);

        let (width, height) = self.default_size();

        settings.set_int("window-width", width)?;
        settings.set_int("window-height", height)?;

        settings.set_boolean("is-maximized", self.is_maximized())?;

        Ok(())
    }

    fn load_window_size(&self) {
        let settings = gio::Settings::new(APP_ID);

        let width = settings.int("window-width");
        let height = settings.int("window-height");
        let is_maximized = settings.boolean("is-maximized");

        self.set_default_size(width, height);

        if is_maximized {
            self.maximize();
        }
    }

    fn queue_draw_graph(&self) {
        let imp = self.imp();
        imp.queued_draw_graph.set(true);
        imp.spinner_revealer.set_reveal_child(true);
    }

    async fn start_draw_graph_loop(&self) {
        let imp = self.imp();

        loop {
            glib::timeout_future(DRAW_GRAPH_INTERVAL).await;

            if !imp.queued_draw_graph.get() {
                continue;
            }

            imp.queued_draw_graph.set(false);

            match self.draw_graph().await {
                Ok(texture) => {
                    imp.picture.set_paintable(texture.as_ref());
                    tracing::debug!("Graph updated");
                }
                Err(err) => {
                    imp.picture.set_paintable(gdk::Paintable::NONE);
                    tracing::error!("Failed to draw graph: {:?}", err);
                }
            }

            imp.spinner_revealer.set_reveal_child(false);
        }
    }

    async fn draw_graph(&self) -> Result<Option<gdk::Texture>> {
        let imp = self.imp();

        let contents = imp
            .buffer
            .text(&imp.buffer.start_iter(), &imp.buffer.end_iter(), false);

        if contents.is_empty() {
            return Ok(None);
        }

        let selected_item = imp
            .layout_drop_down
            .selected_item()
            .unwrap()
            .downcast::<adw::EnumListItem>()
            .unwrap();
        let selected_layout = Layout::try_from(selected_item.value()).unwrap();

        let png_bytes = graphviz::run(contents.as_bytes(), selected_layout, Format::Svg).await?;

        let texture =
            gio::spawn_blocking(|| gdk::Texture::from_bytes(&glib::Bytes::from_owned(png_bytes)))
                .await
                .unwrap()?;

        Ok(Some(texture))
    }

    fn update_buffer_style_scheme(&self) {
        let imp = self.imp();

        let style_manager = adw::StyleManager::default();
        let style_scheme_manager = gtk_source::StyleSchemeManager::default();

        let style_scheme = if style_manager.is_dark() {
            style_scheme_manager
                .scheme("Adwaita-dark")
                .or_else(|| style_scheme_manager.scheme("classic-dark"))
        } else {
            style_scheme_manager
                .scheme("Adwaita")
                .or_else(|| style_scheme_manager.scheme("classic"))
        };

        imp.buffer.set_style_scheme(style_scheme.as_ref());
    }
}
