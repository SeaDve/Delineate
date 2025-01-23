use std::sync::LazyLock;

use fuzzy_matcher::{skim::SkimMatcherV2, FuzzyMatcher};
use gtk::{gio, glib, prelude::*, subclass::prelude::*};

static FUZZY_MATCHER: LazyLock<SkimMatcherV2> = LazyLock::new(SkimMatcherV2::default);

mod imp {
    use std::cell::{OnceCell, RefCell};

    use super::*;

    #[derive(glib::Properties)]
    #[properties(wrapper_type = super::RecentItem)]
    pub struct RecentItem {
        #[property(get, set, construct_only)]
        pub(super) file: OnceCell<gio::File>,
        #[property(get, set = Self::set_added, explicit_notify, construct)]
        pub(super) added: RefCell<glib::DateTime>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for RecentItem {
        const NAME: &'static str = "DelineateRecentItem";
        type Type = super::RecentItem;

        fn new() -> Self {
            Self {
                file: OnceCell::new(),
                // It would panic if RecentItem is constructed without `added`, so this
                // is never actually accessed.
                added: RefCell::new(glib::DateTime::from_unix_utc(0).unwrap()),
            }
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for RecentItem {}

    impl RecentItem {
        fn set_added(&self, added: glib::DateTime) {
            let obj = self.obj();

            if added == obj.added() {
                return;
            }

            self.added.replace(added);
            obj.notify_added();
        }
    }
}

glib::wrapper! {
    pub struct RecentItem(ObjectSubclass<imp::RecentItem>);
}

impl RecentItem {
    pub fn new(file: &gio::File, added: &glib::DateTime) -> Self {
        glib::Object::builder()
            .property("file", file)
            .property("added", added)
            .build()
    }

    pub fn fuzzy_match(&self, pattern: &str) -> Option<i64> {
        let choice = self.file().path().unwrap();
        FUZZY_MATCHER.fuzzy_match(choice.to_string_lossy().trim_end_matches(".gv"), pattern)
    }
}
