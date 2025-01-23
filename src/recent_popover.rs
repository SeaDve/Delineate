use gtk::{
    glib::{self, clone, closure_local},
    prelude::*,
    subclass::prelude::*,
};

use crate::{
    recent_filter::RecentFilter, recent_item::RecentItem, recent_list::RecentList,
    recent_row::RecentRow, recent_sorter::RecentSorter, session::Session,
};

mod imp {
    use std::{cell::OnceCell, sync::LazyLock};

    use glib::subclass::Signal;

    use super::*;

    #[derive(Default, gtk::CompositeTemplate)]
    #[template(resource = "/io/github/seadve/Delineate/ui/recent_popover.ui")]
    pub struct RecentPopover {
        #[template_child]
        pub(super) search_entry: TemplateChild<gtk::SearchEntry>,
        #[template_child]
        pub(super) stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub(super) loading_page: TemplateChild<adw::Spinner>,
        #[template_child]
        pub(super) empty_page: TemplateChild<adw::StatusPage>,
        #[template_child]
        pub(super) empty_search_page: TemplateChild<adw::StatusPage>,
        #[template_child]
        pub(super) list_page: TemplateChild<gtk::ScrolledWindow>,
        #[template_child]
        pub(super) list_box: TemplateChild<gtk::ListBox>,

        pub(super) model: OnceCell<RecentList>,
        pub(super) filter_model: OnceCell<gtk::FilterListModel>,
        pub(super) sort_model: OnceCell<gtk::SortListModel>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for RecentPopover {
        const NAME: &'static str = "DelineateRecentPopover";
        type Type = super::RecentPopover;
        type ParentType = gtk::Popover;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for RecentPopover {
        fn constructed(&self) {
            self.parent_constructed();

            let obj = self.obj();

            self.list_box.connect_row_activated(clone!(
                #[weak]
                obj,
                move |_, row| {
                    let row = row.downcast_ref::<RecentRow>().unwrap();
                    obj.emit_item_activated(&row.item());
                    obj.popdown();
                }
            ));

            self.search_entry.connect_stop_search(clone!(
                #[weak]
                obj,
                move |_| {
                    obj.popdown();
                }
            ));
            self.search_entry.connect_activate(clone!(
                #[weak]
                obj,
                move |_| {
                    let imp = obj.imp();
                    if let Some(item) = imp
                        .sort_model
                        .get()
                        .and_then(|sort_model| sort_model.item(0))
                    {
                        let item = item.downcast_ref().unwrap();
                        obj.emit_item_activated(item);
                        obj.popdown();
                    }
                }
            ));

            obj.update_search_entry_sensitivity();
            obj.update_stack();
        }

        fn signals() -> &'static [Signal] {
            static SIGNALS: LazyLock<Vec<Signal>> = LazyLock::new(|| {
                vec![Signal::builder("item-activated")
                    .param_types([RecentItem::static_type()])
                    .build()]
            });

            SIGNALS.as_ref()
        }
    }

    impl WidgetImpl for RecentPopover {}

    impl PopoverImpl for RecentPopover {
        fn closed(&self) {
            self.search_entry.set_text("");
        }
    }
}

glib::wrapper! {
    pub struct RecentPopover(ObjectSubclass<imp::RecentPopover>)
        @extends gtk::Widget, gtk::Popover;
}

impl RecentPopover {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn connect_item_activated<F>(&self, f: F) -> glib::SignalHandlerId
    where
        F: Fn(&Self, &RecentItem) + 'static,
    {
        self.connect_closure(
            "item-activated",
            false,
            closure_local!(|obj: &Self, item: &RecentItem| {
                f(obj, item);
            }),
        )
    }

    /// This must only be called once
    pub fn bind_model(&self, model: &RecentList) {
        let imp = self.imp();

        model.connect_items_changed(clone!(
            #[weak(rename_to = obj)]
            self,
            move |_, _, _, _| {
                obj.update_search_entry_sensitivity();
                obj.update_stack();
            }
        ));
        imp.model.set(model.clone()).unwrap();

        let filter = RecentFilter::new();
        let sorter = RecentSorter::new();
        imp.search_entry.connect_search_changed(clone!(
            #[weak(rename_to = obj)]
            self,
            #[weak]
            filter,
            #[weak]
            sorter,
            move |search_entry| {
                let text = search_entry.text();
                filter.set_search(text.trim());
                sorter.set_search(text.trim());
                obj.update_stack();
            }
        ));

        let filter_model = gtk::FilterListModel::new(Some(model.clone()), Some(filter));
        filter_model.connect_items_changed(clone!(
            #[weak(rename_to = obj)]
            self,
            move |_, _, _, _| {
                obj.update_stack();
            }
        ));
        imp.filter_model.set(filter_model.clone()).unwrap();

        let sort_model = gtk::SortListModel::new(Some(filter_model), Some(sorter));
        imp.sort_model.set(sort_model.clone()).unwrap();

        imp.list_box.bind_model(
            Some(&sort_model),
            clone!(
                #[weak(rename_to = obj)]
                self,
                #[upgrade_or_panic]
                move |item| obj.create_row(item.downcast_ref().unwrap()).upcast()
            ),
        );

        self.update_search_entry_sensitivity();
        self.update_stack();
    }

    /// Shows the loading page until the model is bound.
    pub fn begin_loading(&self) {
        let imp = self.imp();

        imp.stack.set_visible_child(&*imp.loading_page);
    }

    fn emit_item_activated(&self, item: &RecentItem) {
        self.emit_by_name::<()>("item-activated", &[item]);
    }

    fn create_row(&self, item: &RecentItem) -> RecentRow {
        let item = item.downcast_ref().unwrap();
        let row = RecentRow::new(item);
        row.connect_remove_request(clone!(
            #[weak(rename_to = obj)]
            self,
            move |row| {
                let imp = obj.imp();

                let uri = row.item().file().uri();
                imp.model.get().unwrap().remove(&uri);

                let session = Session::instance();
                session.mark_dirty();
            }
        ));
        row.upcast()
    }

    fn update_search_entry_sensitivity(&self) {
        let imp = self.imp();

        let has_items = imp.model.get().is_some_and(|model| model.n_items() != 0);
        imp.search_entry.set_sensitive(has_items);
    }

    fn update_stack(&self) {
        let imp = self.imp();

        let search_text = imp.search_entry.text();

        if imp
            .filter_model
            .get()
            .map_or(true, |filter_model| filter_model.n_items() == 0)
            && !search_text.is_empty()
        {
            imp.stack.set_visible_child(&*imp.empty_search_page);
        } else if imp.model.get().map_or(true, |model| model.n_items() == 0)
            && search_text.is_empty()
        {
            imp.stack.set_visible_child(&*imp.empty_page);
        } else {
            imp.stack.set_visible_child(&*imp.list_page);
        }
    }
}
