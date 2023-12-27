use gtk::{
    glib::{self, closure_local},
    prelude::*,
    subclass::prelude::*,
};

use crate::{recent_item::RecentItem, utils};

mod imp {
    use std::cell::OnceCell;

    use glib::{once_cell::sync::Lazy, subclass::Signal};

    use super::*;

    #[derive(Default, glib::Properties, gtk::CompositeTemplate)]
    #[properties(wrapper_type = super::RecentRow)]
    #[template(resource = "/io/github/seadve/Dagger/ui/recent_row.ui")]
    pub struct RecentRow {
        #[property(get, set, construct)]
        pub(super) item: OnceCell<RecentItem>,

        #[template_child]
        pub(super) title_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub(super) subtitle_label: TemplateChild<gtk::Label>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for RecentRow {
        const NAME: &'static str = "DaggerRecentRow";
        type Type = super::RecentRow;
        type ParentType = gtk::ListBoxRow;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();

            klass.install_action("recent-row.remove", None, |obj, _, _| {
                obj.emit_by_name::<()>("remove-request", &[]);
            });
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for RecentRow {
        fn constructed(&self) {
            self.parent_constructed();

            let obj = self.obj();
            let item = obj.item();
            let file = item.file();

            obj.set_tooltip_text(Some(&utils::display_file(&file)));

            self.title_label.set_label(&utils::display_file_stem(&file));
            self.subtitle_label
                .set_label(&utils::display_file_parent(&file));
        }

        fn signals() -> &'static [Signal] {
            static SIGNALS: Lazy<Vec<Signal>> =
                Lazy::new(|| vec![Signal::builder("remove-request").build()]);

            SIGNALS.as_ref()
        }
    }

    impl WidgetImpl for RecentRow {}
    impl ListBoxRowImpl for RecentRow {}
}

glib::wrapper! {
    pub struct RecentRow(ObjectSubclass<imp::RecentRow>)
        @extends gtk::Widget, gtk::ListBoxRow;
}

impl RecentRow {
    pub fn new(item: &RecentItem) -> Self {
        glib::Object::builder().property("item", item).build()
    }

    pub fn connect_remove_request<F>(&self, f: F) -> glib::SignalHandlerId
    where
        F: Fn(&Self) + 'static,
    {
        self.connect_closure(
            "remove-request",
            true,
            closure_local!(|obj: &Self| {
                f(obj);
            }),
        )
    }
}
