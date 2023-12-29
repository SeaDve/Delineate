use std::time::Duration;

use gettextrs::gettext;
use gtk::{
    glib::{self, clone, closure_local, TimeSpan},
    prelude::*,
    subclass::prelude::*,
};

use crate::{i18n::ngettext_f, recent_item::RecentItem, utils};

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
        #[template_child]
        pub(super) age_label: TemplateChild<gtk::Label>,
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

            // Update age label every 30 minutes.
            glib::timeout_add_local_full(
                Duration::from_secs(60 * 30),
                glib::Priority::LOW,
                clone!(@weak obj => @default-panic, move || {
                    obj.update_age_label();
                    glib::ControlFlow::Continue
                }),
            );

            obj.update_age_label();
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
            false,
            closure_local!(|obj: &Self| {
                f(obj);
            }),
        )
    }

    fn update_age_label(&self) {
        let imp = self.imp();

        let added = self.item().added();

        let now = glib::DateTime::now_utc().unwrap();
        let diff = now.difference(&added);

        // Copied from GNOME Text Editor's `_editor_date_time_format`
        let label = if diff < TimeSpan(0) {
            "".to_string()
        } else if diff < TimeSpan::from_minutes(45) {
            gettext("Just Now")
        } else if diff < TimeSpan::from_minutes(90) {
            gettext("An hour ago")
        } else if diff < TimeSpan::from_days(2) {
            gettext("Yesterday")
        } else if diff < TimeSpan::from_days(7) {
            added.format("%A").unwrap().to_string()
        } else if diff < TimeSpan::from_days(365) {
            added.format("%B %e").unwrap().to_string()
        } else if diff < TimeSpan::from_days(365 + 365 / 2) {
            gettext("About a year ago")
        } else {
            let n_years = diff.as_days() / 365;
            ngettext_f(
                "About {n_years} year ago",
                "About {n_years} years ago",
                n_years as u32,
                &[("n_years", &n_years.to_string())],
            )
        };
        imp.age_label.set_label(&label);
    }
}
