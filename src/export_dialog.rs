use adw::{prelude::*, subclass::prelude::*};
use futures_channel::oneshot;
use gtk::glib::{self, closure};

use crate::{cancelled::Cancelled, graphviz::Format};

mod imp {
    use std::cell::RefCell;

    use super::*;

    #[derive(Default, gtk::CompositeTemplate)]
    #[template(resource = "/io/github/seadve/Dagger/ui/export_dialog.ui")]
    pub struct ExportDialog {
        #[template_child]
        pub(super) format_row: TemplateChild<adw::ComboRow>,

        pub(super) tx: RefCell<Option<oneshot::Sender<()>>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ExportDialog {
        const NAME: &'static str = "DaggerExportDialog";
        type Type = super::ExportDialog;
        type ParentType = adw::Window;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();

            klass.install_action("export-dialog.export", None, |obj, _, _| {
                let tx = obj.imp().tx.take().unwrap();
                tx.send(()).unwrap();
            });
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for ExportDialog {
        fn constructed(&self) {
            self.parent_constructed();

            self.format_row
                .set_expression(Some(&gtk::ClosureExpression::new::<glib::GString>(
                    &[] as &[gtk::Expression],
                    closure!(|list_item: adw::EnumListItem| {
                        let format = Format::try_from(list_item.value()).unwrap();
                        format.name()
                    }),
                )));
            self.format_row
                .set_model(Some(&adw::EnumListModel::new(Format::static_type())));
        }

        fn dispose(&self) {
            self.dispose_template();
        }
    }

    impl WidgetImpl for ExportDialog {}

    impl WindowImpl for ExportDialog {
        fn close_request(&self) -> glib::Propagation {
            let _ = self.tx.take();

            self.parent_close_request()
        }
    }

    impl AdwWindowImpl for ExportDialog {}
}

glib::wrapper! {
    pub struct ExportDialog(ObjectSubclass<imp::ExportDialog>)
        @extends gtk::Widget, gtk::Window, adw::Window;
}

impl ExportDialog {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub async fn export(self) -> Result<Format, Cancelled> {
        let imp = self.imp();

        let (tx, rx) = oneshot::channel();

        imp.tx.replace(Some(tx));

        self.present();

        rx.await.map_err(|_| Cancelled)?;

        let selected_item = imp
            .format_row
            .selected_item()
            .unwrap()
            .downcast::<adw::EnumListItem>()
            .unwrap();
        let ret = Format::try_from(selected_item.value()).unwrap();

        self.close();

        Ok(ret)
    }
}

impl Default for ExportDialog {
    fn default() -> Self {
        Self::new()
    }
}
