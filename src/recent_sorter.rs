use gtk::{glib, prelude::*, subclass::prelude::*};

use std::cell::RefCell;

use crate::recent_item::RecentItem;

mod imp {
    use super::*;

    #[derive(Default, glib::Properties)]
    #[properties(wrapper_type = super::RecentSorter)]
    pub struct RecentSorter {
        #[property(get, set = Self::set_search, explicit_notify)]
        pub(super) search: RefCell<String>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for RecentSorter {
        const NAME: &'static str = "DaggerRecentSorter";
        type Type = super::RecentSorter;
        type ParentType = gtk::Sorter;
    }

    #[glib::derived_properties]
    impl ObjectImpl for RecentSorter {}

    impl SorterImpl for RecentSorter {
        fn compare(&self, item_1: &glib::Object, item_2: &glib::Object) -> gtk::Ordering {
            let item_1 = item_1.downcast_ref::<RecentItem>().unwrap();
            let item_2 = item_2.downcast_ref::<RecentItem>().unwrap();

            let search = self.search.borrow();

            if search.is_empty() {
                item_2.added().cmp(&item_1.added()).into()
            } else {
                let score_1 = item_1.fuzzy_match(&search);
                let score_2 = item_2.fuzzy_match(&search);
                score_2.cmp(&score_1).into()
            }
        }

        fn order(&self) -> gtk::SorterOrder {
            gtk::SorterOrder::Partial
        }
    }

    impl RecentSorter {
        fn set_search(&self, search: String) {
            let obj = self.obj();

            if search == obj.search() {
                return;
            }

            self.search.replace(search);
            obj.changed(gtk::SorterChange::Different);
            obj.notify_search();
        }
    }
}

glib::wrapper! {
    pub struct RecentSorter(ObjectSubclass<imp::RecentSorter>)
        @extends gtk::Sorter;

}

impl RecentSorter {
    pub fn new() -> Self {
        glib::Object::new()
    }
}
