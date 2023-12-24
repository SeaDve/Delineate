use std::{cell::RefCell, error, fmt, path::Path, rc::Rc};

use adw::prelude::*;
use anyhow::Result;
use gettextrs::{gettext, ngettext};
use gtk::{
    gio,
    glib::{self, clone},
};

use crate::{document::Document, i18n::gettext_f, window::Window};

const CANCEL_RESPONSE_ID: &str = "cancel";
const DISCARD_RESPONSE_ID: &str = "discard";
const SAVE_RESPONSE_ID: &str = "save";

/// Indicates that the user cancelled the operation.
#[derive(Debug)]
struct Cancelled;

impl fmt::Display for Cancelled {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Task cancelled")
    }
}

impl error::Error for Cancelled {}

struct SaveFileItem<'a> {
    document: &'a Document,
    check_button: gtk::CheckButton,
    /// Only for draft (new) documents
    save_as_file: Option<gio::File>,
}

/// Returns `Proceed` if unsaved changes are handled and can proceed, `Stop` if
/// the next operation should be aborted.
pub async fn run(window: &Window, unsaved: &[Document]) -> glib::Propagation {
    match run_inner(window, unsaved).await {
        Ok(_) => glib::Propagation::Proceed,
        Err(err) => {
            if !err.is::<Cancelled>() {
                tracing::error!("Failed to save changes to document: {:?}", err);
                window.add_message_toast(&gettext("Failed to save changes to document"));
            }
            glib::Propagation::Stop
        }
    }
}

/// Returns `Ok` if unsaved changes are handled and can proceed, `Err` if
/// the next operation should be aborted.
async fn run_inner(parent: &impl IsA<gtk::Window>, unsaved: &[Document]) -> Result<()> {
    debug_assert!(!unsaved.is_empty());

    let dialog = adw::MessageDialog::builder()
        .modal(true)
        .transient_for(parent)
        .heading(gettext("Save Changes?"))
        .body(gettext("Open documents contain unsaved changes. Changes which are not saved will be permanently lost."))
        .close_response(CANCEL_RESPONSE_ID)
        .default_response(SAVE_RESPONSE_ID)
        .build();
    dialog.add_css_class("save-changes-dialog");

    dialog.add_response(CANCEL_RESPONSE_ID, &gettext("Cancel"));
    dialog.add_response(
        DISCARD_RESPONSE_ID,
        &ngettext("_Discard", "_Discard All", unsaved.len() as u32),
    );
    dialog.add_response(
        SAVE_RESPONSE_ID,
        &ngettext("_Save", "_Save All", unsaved.len() as u32),
    );

    dialog.set_response_appearance(DISCARD_RESPONSE_ID, adw::ResponseAppearance::Destructive);
    dialog.set_response_appearance(SAVE_RESPONSE_ID, adw::ResponseAppearance::Suggested);

    let page = adw::PreferencesPage::new();
    dialog.set_extra_child(Some(&page));

    let group = adw::PreferencesGroup::new();
    page.add(&group);

    let mut items = Vec::new();
    let check_buttons = Rc::new(RefCell::new(Vec::new()));
    for document in unsaved {
        debug_assert!(document.is_modified());

        let row = adw::ActionRow::new();
        group.add(&row);

        let check_button = gtk::CheckButton::builder()
            .valign(gtk::Align::Center)
            .active(true)
            .build();
        row.add_prefix(&check_button);
        row.set_activatable_widget(Some(&check_button));
        check_buttons.borrow_mut().push(check_button.clone());

        let title = document.title();

        let item = if let Some(file) = document.file() {
            row.set_title(&title);
            row.set_subtitle(&display_file_parent(&file));

            SaveFileItem {
                document,
                check_button,
                save_as_file: None,
            }
        } else {
            let title = if title.is_empty() {
                gettext("Untitled Document")
            } else {
                title
            };
            row.set_title(&gettext_f("{title} (new)", &[("title", &title)]));

            let file = {
                let mut path = glib::user_special_dir(glib::UserDirectory::Documents)
                    .unwrap_or_else(glib::home_dir);
                path.push(title);
                path.set_extension("gv");

                gio::File::for_path(path)
            };
            row.set_subtitle(&display_file_parent(&file));

            SaveFileItem {
                document,
                check_button,
                save_as_file: Some(file),
            }
        };

        items.push(item);
    }

    for button in check_buttons.borrow().iter() {
        button.connect_active_notify(clone!(@weak dialog, @weak check_buttons => move |_| {
            let n_active = check_buttons
                .borrow()
                .iter()
                .filter(|b| b.is_active())
                .count();
            dialog.set_response_enabled(SAVE_RESPONSE_ID, n_active != 0);
            dialog.set_response_label(
                SAVE_RESPONSE_ID,
                &ngettext("_Save", "_Save All", n_active as u32),
            );
        }));
    }

    match dialog.choose_future().await.as_str() {
        CANCEL_RESPONSE_ID => Err(Cancelled.into()),
        DISCARD_RESPONSE_ID => Ok(()),
        SAVE_RESPONSE_ID => {
            for item in items {
                let SaveFileItem {
                    document,
                    check_button,
                    save_as_file,
                } = item;

                if !check_button.is_active() {
                    continue;
                }

                if let Some(file) = save_as_file {
                    document.save_as(&file).await?;
                } else {
                    document.save().await?;
                }
            }

            Ok(())
        }
        _ => unreachable!(),
    }
}

fn display_file_parent(file: &gio::File) -> String {
    if let Some(parent) = file.parent() {
        if parent.is_native() {
            display_path(&parent.path().unwrap())
        } else {
            parent.uri().to_string()
        }
    } else {
        "/".to_string()
    }
}

fn display_path(path: &Path) -> String {
    let home_dir = glib::home_dir();

    let path_display = path.display().to_string();
    let home_dir_display = home_dir.display().to_string();

    if path == home_dir {
        return "~/".to_string();
    }

    if path.starts_with(&home_dir) {
        return format!("~{}", &path_display[home_dir_display.len()..]);
    }

    path_display
}