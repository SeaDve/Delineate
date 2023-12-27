use gtk::{glib, prelude::*, subclass::prelude::*};

use std::cell::RefCell;

use crate::recent_item::RecentItem;

mod imp {
    use super::*;

    #[derive(Default, glib::Properties)]
    #[properties(wrapper_type = super::RecentFilter)]
    pub struct RecentFilter {
        #[property(get, set = Self::set_search, explicit_notify)]
        pub(super) search: RefCell<String>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for RecentFilter {
        const NAME: &'static str = "DaggerRecentFilter";
        type Type = super::RecentFilter;
        type ParentType = gtk::Filter;
    }

    #[glib::derived_properties]
    impl ObjectImpl for RecentFilter {}

    impl FilterImpl for RecentFilter {
        fn strictness(&self) -> gtk::FilterMatch {
            if self.search.borrow().is_empty() {
                gtk::FilterMatch::All
            } else {
                gtk::FilterMatch::Some
            }
        }

        fn match_(&self, item: &glib::Object) -> bool {
            let item = item.downcast_ref::<RecentItem>().unwrap();

            let search = self.search.borrow();

            if search.is_empty() {
                true
            } else {
                item.fuzzy_match(&search).is_some()
            }
        }
    }

    impl RecentFilter {
        fn set_search(&self, search: &str) {
            let obj = self.obj();
            let old_search = obj.search();
            let search = search.to_lowercase();

            if old_search == search {
                return;
            }

            let change = if search.is_empty() {
                gtk::FilterChange::LessStrict
            } else if search.starts_with(&old_search) {
                gtk::FilterChange::MoreStrict
            } else if old_search.starts_with(&search) {
                gtk::FilterChange::LessStrict
            } else {
                gtk::FilterChange::Different
            };

            self.search.replace(search);
            obj.changed(change);
            obj.notify_search();
        }
    }
}

glib::wrapper! {
    pub struct RecentFilter(ObjectSubclass<imp::RecentFilter>)
        @extends gtk::Filter;

}

impl RecentFilter {
    pub fn new() -> Self {
        glib::Object::new()
    }
}
