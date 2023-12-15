use gtk::{
    gdk,
    glib::{self, clone},
    graphene::Point,
    prelude::*,
    subclass::prelude::*,
};
use gtk_source::{prelude::*, subclass::prelude::*};

use crate::colors::{RED_1, RED_4};

const SIZE_SP: f64 = 12.0;

mod imp {
    use std::{
        cell::{Cell, RefCell},
        collections::HashMap,
    };

    use super::*;

    #[derive(Default, glib::Properties)]
    #[properties(wrapper_type = super::ErrorGutterRenderer)]
    pub struct ErrorGutterRenderer {
        #[property(get)]
        pub(super) has_visible_errors: Cell<bool>,

        pub(super) error_lines: RefCell<HashMap<u32, String>>,
        pub(super) paintable: RefCell<Option<gtk::IconPaintable>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ErrorGutterRenderer {
        const NAME: &'static str = "DaggerErrorGutterRenderer";
        type Type = super::ErrorGutterRenderer;
        type ParentType = gtk_source::GutterRenderer;
    }

    #[glib::derived_properties]
    impl ObjectImpl for ErrorGutterRenderer {
        fn constructed(&self) {
            self.parent_constructed();

            let obj = self.obj();
            obj.set_has_tooltip(true);
            obj.set_yalign(0.5);

            obj.connect_scale_factor_notify(clone!(@weak obj => move |_| {
                obj.cache_paintable();
            }));

            obj.settings()
                .connect_gtk_xft_dpi_notify(clone!(@weak obj => move |_| {
                    obj.cache_paintable();
                }));

            obj.cache_paintable();
        }
    }

    impl WidgetImpl for ErrorGutterRenderer {
        fn measure(&self, _orientation: gtk::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            match _orientation {
                gtk::Orientation::Horizontal => {
                    let size = self.obj().size() as i32;
                    (size, size, -1, -1)
                }
                gtk::Orientation::Vertical => (0, 0, -1, -1),
                _ => unreachable!(),
            }
        }

        fn query_tooltip(
            &self,
            _x: i32,
            y: i32,
            _keyboard_tooltip: bool,
            tooltip: &gtk::Tooltip,
        ) -> bool {
            let obj = self.obj();

            let view = obj.view();
            let (_, buffer_y) = view.window_to_buffer_coords(gtk::TextWindowType::Left, 0, y);
            let (iter, _) = view.line_at_y(buffer_y);
            let line = iter.line() as u32;

            if let Some(message) = self.error_lines.borrow().get(&line) {
                tooltip.set_text(Some(message));
                return true;
            }

            false
        }
    }

    impl GutterRendererImpl for ErrorGutterRenderer {
        fn begin(&self, lines: &gtk_source::GutterLines) {
            self.parent_begin(lines);

            let obj = self.obj();

            let visible_line_range = lines.first()..=lines.last();

            let has_visible_errors = self
                .error_lines
                .borrow()
                .keys()
                .any(|line| visible_line_range.contains(line));
            obj.set_has_visible_errors(has_visible_errors);
        }

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

            if self.error_lines.borrow().contains_key(&line) {
                let size = obj.size();
                let (x, y) = obj.align_cell(line, size as f32, size as f32);

                snapshot.save();
                snapshot.translate(&Point::new(x, y));

                let style_manager = adw::StyleManager::default();
                let color = if style_manager.is_dark() {
                    RED_1
                } else {
                    RED_4
                };

                self.paintable.borrow().as_ref().unwrap().snapshot_symbolic(
                    snapshot,
                    size,
                    size,
                    &[color],
                );

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

    pub fn set_error(&self, line: u32, message: impl Into<String>) {
        self.imp()
            .error_lines
            .borrow_mut()
            .insert(line, message.into());
        self.queue_draw();
    }

    pub fn clear_errors(&self) {
        self.imp().error_lines.borrow_mut().clear();
        self.queue_draw();
    }

    fn size(&self) -> f64 {
        adw::LengthUnit::Sp.to_px(SIZE_SP, Some(&self.settings()))
    }

    fn set_has_visible_errors(&self, has_visible_errors: bool) {
        if has_visible_errors == self.has_visible_errors() {
            return;
        }

        self.imp().has_visible_errors.set(has_visible_errors);
        self.notify_has_visible_errors();
    }

    fn cache_paintable(&self) {
        let imp = self.imp();

        let icon_theme = gtk::IconTheme::for_display(&self.display());
        let paintable = icon_theme.lookup_icon(
            "error-symbolic",
            &[],
            self.size() as i32,
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
