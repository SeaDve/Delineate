use gtk::{
    glib::{self, clone},
    prelude::*,
    subclass::prelude::*,
};

mod imp {
    use std::{cell::RefCell, marker::PhantomData};

    use super::*;

    #[derive(Default, glib::Properties, gtk::CompositeTemplate)]
    #[properties(wrapper_type = super::DragOverlay)]
    #[template(resource = "/io/github/seadve/Delineate/ui/drag_overlay.ui")]
    pub struct DragOverlay {
        #[property(get = Self::child, set = Self::set_child, nullable)]
        pub(super) child: PhantomData<Option<gtk::Widget>>,
        #[property(get, set = Self::set_target, explicit_notify, nullable)]
        pub(super) target: RefCell<Option<gtk::DropTarget>>,

        #[template_child]
        pub(super) overlay: TemplateChild<gtk::Overlay>,
        #[template_child]
        pub(super) revealer: TemplateChild<gtk::Revealer>,

        pub(super) target_handler_id: RefCell<Option<glib::SignalHandlerId>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for DragOverlay {
        const NAME: &'static str = "DelineateDragOverlay";
        type Type = super::DragOverlay;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.set_css_name("dragoverlay");

            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for DragOverlay {
        fn dispose(&self) {
            self.dispose_template();
        }
    }

    impl WidgetImpl for DragOverlay {}

    impl DragOverlay {
        fn child(&self) -> Option<gtk::Widget> {
            self.overlay.child()
        }

        fn set_child(&self, child: Option<&gtk::Widget>) {
            self.overlay.set_child(child);
        }

        fn set_target(&self, target: Option<gtk::DropTarget>) {
            let obj = self.obj();

            if let Some(prev_target) = self.target.take() {
                obj.remove_controller(&prev_target);

                let handler_id = self.target_handler_id.take().unwrap();
                prev_target.disconnect(handler_id);
            }

            if let Some(ref target) = target {
                let handler_id = target.connect_current_drop_notify(clone!(
                    #[weak]
                    obj,
                    move |target| {
                        obj.imp()
                            .revealer
                            .set_reveal_child(target.current_drop().is_some());
                    }
                ));
                self.target_handler_id.replace(Some(handler_id));

                obj.add_controller(target.clone());
            }

            self.target.replace(target);
            obj.notify_target();
        }
    }
}

glib::wrapper! {
    pub struct DragOverlay(ObjectSubclass<imp::DragOverlay>)
        @extends gtk::Widget;
}

impl DragOverlay {
    pub fn new() -> Self {
        glib::Object::new()
    }
}
