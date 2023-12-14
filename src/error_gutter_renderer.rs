use gtk::{
    gdk,
    glib::{self, clone},
    graphene::Point,
    prelude::*,
    subclass::prelude::*,
};
use gtk_source::{prelude::*, subclass::prelude::*};

use crate::colors::{RED_1, RED_4};

const CELL_SIZE: i32 = 12;

mod imp {
    use std::cell::{OnceCell, RefCell};

    use super::*;

    #[derive(Default)]
    pub struct ErrorGutterRenderer {
        pub(super) error_lines: OnceCell<gtk::Bitset>,

        pub(super) paintable: RefCell<Option<gtk::IconPaintable>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ErrorGutterRenderer {
        const NAME: &'static str = "DaggerErrorGutterRenderer";
        type Type = super::ErrorGutterRenderer;
        type ParentType = gtk_source::GutterRenderer;
    }

    impl ObjectImpl for ErrorGutterRenderer {
        fn constructed(&self) {
            self.parent_constructed();

            self.error_lines.set(gtk::Bitset::new_empty()).unwrap();

            let obj = self.obj();

            obj.connect_scale_factor_notify(clone!(@weak obj => move |_| {
                obj.cache_paintable();
            }));

            obj.cache_paintable();
        }
    }

    impl WidgetImpl for ErrorGutterRenderer {
        fn measure(&self, _orientation: gtk::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            match _orientation {
                gtk::Orientation::Horizontal => (CELL_SIZE, CELL_SIZE, -1, -1),
                gtk::Orientation::Vertical => (0, 0, -1, -1),
                _ => unreachable!(),
            }
        }
    }

    impl GutterRendererImpl for ErrorGutterRenderer {
        fn query_activatable(&self, _iter: &gtk::TextIter, _area: &gdk::Rectangle) -> bool {
            false
        }

        fn snapshot_line(
            &self,
            snapshot: &gtk::Snapshot,
            _lines: &gtk_source::GutterLines,
            line: u32,
        ) {
            let obj = self.obj();

            if obj.error_lines().contains(line) {
                let (x, y) = obj.align_cell(line, CELL_SIZE as f32, CELL_SIZE as f32);

                snapshot.save();
                snapshot.translate(&Point::new(x, y + 2.0));

                let paintable = self.paintable.borrow();
                let paintable = paintable.as_ref().unwrap();

                let style_manager = adw::StyleManager::default();
                let color = if style_manager.is_dark() {
                    RED_1
                } else {
                    RED_4
                };

                paintable.snapshot_symbolic(snapshot, CELL_SIZE as f64, CELL_SIZE as f64, &[color]);

                snapshot.restore();
            }
        }
    }
}

glib::wrapper! {
    pub struct ErrorGutterRenderer(ObjectSubclass<imp::ErrorGutterRenderer>)
        @extends gtk::Widget, gtk_source::GutterRenderer;
}

impl ErrorGutterRenderer {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_error(&self, line: u32) {
        self.error_lines().add(line);
        self.queue_draw();
    }

    pub fn clear_errors(&self) {
        self.error_lines().remove_all();
        self.queue_draw();
    }

    fn error_lines(&self) -> &gtk::Bitset {
        self.imp().error_lines.get().unwrap()
    }

    fn cache_paintable(&self) {
        let imp = self.imp();

        let icon_theme = gtk::IconTheme::for_display(&self.display());
        let paintable = icon_theme.lookup_icon(
            "error-symbolic",
            &[],
            CELL_SIZE,
            self.scale_factor(),
            self.direction(),
            gtk::IconLookupFlags::FORCE_SYMBOLIC,
        );
        imp.paintable.replace(Some(paintable));
    }
}

impl Default for ErrorGutterRenderer {
    fn default() -> Self {
        Self::new()
    }
}
