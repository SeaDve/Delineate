use std::{future::Future, pin::Pin};

use anyhow::{ensure, Result};
use futures_util::{join, Stream, StreamExt};
use gtk::{
    gio,
    glib::{self, clone},
    prelude::*,
    subclass::prelude::*,
};
use gtk_source::{prelude::*, subclass::prelude::*};

mod imp {
    use std::{cell::Cell, marker::PhantomData};

    use glib::{once_cell::sync::Lazy, subclass::Signal};

    use super::*;

    #[derive(Default, glib::Properties)]
    #[properties(wrapper_type = super::Document)]
    pub struct Document {
        #[property(get = Self::file, set = Self::set_file, construct_only)]
        pub(super) file: PhantomData<Option<gio::File>>,
        #[property(get = Self::title)]
        pub(super) title: PhantomData<String>,
        #[property(get = Self::is_modified)]
        pub(super) is_modified: PhantomData<bool>,
        #[property(get, default_value = 1.0, minimum = 0.0, maximum = 1.0)]
        pub(super) busy_progress: Cell<f64>,

        pub(super) source_file: gtk_source::File,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Document {
        const NAME: &'static str = "DaggerDocument";
        type Type = super::Document;
        type ParentType = gtk_source::Buffer;

        fn new() -> Self {
            Self {
                busy_progress: Cell::new(1.0),
                ..Default::default()
            }
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for Document {
        fn constructed(&self) {
            self.parent_constructed();

            let obj = self.obj();

            obj.connect_loading_notify(clone!(@weak obj => move |_| {
                obj.notify_is_modified();
            }));

            let language_manager = gtk_source::LanguageManager::default();
            if let Some(language) = language_manager.language("dot") {
                obj.set_language(Some(&language));
                obj.set_highlight_syntax(true);
            }

            // FIXME Disable when https://gitlab.gnome.org/World/Rust/sourceview5-rs/-/issues/11 is fixed
            obj.set_highlight_matching_brackets(false);

            let style_manager = adw::StyleManager::default();
            style_manager.connect_dark_notify(clone!(@weak obj => move |_| {
                obj.update_style_scheme();
            }));

            obj.update_style_scheme();
        }

        fn signals() -> &'static [Signal] {
            static SIGNALS: Lazy<Vec<Signal>> =
                Lazy::new(|| vec![Signal::builder("text-changed").build()]);

            SIGNALS.as_ref()
        }
    }

    impl TextBufferImpl for Document {
        fn modified_changed(&self) {
            self.parent_modified_changed();

            self.obj().notify_is_modified();
        }

        fn insert_text(&self, iter: &mut gtk::TextIter, new_text: &str) {
            self.parent_insert_text(iter, new_text);

            let obj = self.obj();

            if obj.file().is_none() {
                obj.notify_title();
            }

            if !obj.is_loading() {
                obj.emit_text_changed();
            }
        }

        fn delete_range(&self, start: &mut gtk::TextIter, end: &mut gtk::TextIter) {
            self.parent_delete_range(start, end);

            let obj = self.obj();

            if obj.file().is_none() {
                obj.notify_title();
            }

            if !obj.is_loading() {
                obj.emit_text_changed();
            }
        }
    }

    impl BufferImpl for Document {}

    impl Document {
        fn file(&self) -> Option<gio::File> {
            // FIXME mark the binding method nullable upstream
            self.source_file.property("location")
        }

        fn set_file(&self, file: Option<&gio::File>) {
            self.source_file.set_location(file);
        }

        fn title(&self) -> String {
            let obj = self.obj();

            if let Some(file) = obj.file() {
                file.path()
                    .unwrap()
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string()
            } else {
                obj.parse_title()
            }
        }

        fn is_modified(&self) -> bool {
            let obj = self.obj();

            // This must not also be loading to be considered modified.
            gtk::TextBuffer::is_modified(obj.upcast_ref()) && !obj.is_loading()
        }
    }
}

glib::wrapper! {
    pub struct Document(ObjectSubclass<imp::Document>)
        @extends gtk::TextBuffer, gtk_source::Buffer;
}

impl Document {
    pub fn draft() -> Self {
        glib::Object::new()
    }

    pub fn for_file(file: gio::File) -> Self {
        glib::Object::builder().property("file", file).build()
    }

    pub fn contents(&self) -> glib::GString {
        self.text(&self.start_iter(), &self.end_iter(), true)
    }

    pub async fn load(&self) -> Result<()> {
        ensure!(self.file().is_some(), "Document must not be a draft");

        let imp = self.imp();

        let loader = gtk_source::FileLoader::new(self, &imp.source_file);
        self.handle_file_io(loader.load_future(glib::Priority::default()))
            .await?;

        self.emit_text_changed();

        Ok(())
    }

    pub async fn save(&self) -> Result<()> {
        ensure!(self.file().is_some(), "Document must not be a draft");

        let imp = self.imp();

        let saver = gtk_source::FileSaver::new(self, &imp.source_file);
        self.handle_file_io(saver.save_future(glib::Priority::default()))
            .await?;

        self.set_modified(false);

        Ok(())
    }

    pub async fn save_as(&self, file: &gio::File) -> Result<()> {
        let imp = self.imp();

        imp.source_file.set_location(Some(file));

        let saver = gtk_source::FileSaver::new(self, &imp.source_file);
        self.handle_file_io(saver.save_future(glib::Priority::default()))
            .await?;

        self.notify_title();

        self.set_modified(false);

        Ok(())
    }

    fn emit_text_changed(&self) {
        self.emit_by_name::<()>("text-changed", &[]);
    }

    fn parse_title(&self) -> String {
        let start = self.start_iter();

        let mut second_word_end = start;
        second_word_end.forward_word_end();
        second_word_end.forward_word_end();

        let search_flags = gtk::TextSearchFlags::CASE_INSENSITIVE
            | gtk::TextSearchFlags::TEXT_ONLY
            | gtk::TextSearchFlags::VISIBLE_ONLY;

        // Second word is either the `digraph`/`graph` keyword or the title.
        let search_match = start
            .forward_search("digraph", search_flags, Some(&second_word_end))
            .or_else(|| start.forward_search("graph", search_flags, Some(&second_word_end)));

        let Some((match_start, match_end)) = search_match else {
            return "".to_string();
        };

        // `digraph` and `graph` must be a standalone word.
        if !match_start.starts_word() || !match_end.ends_word() {
            return "".to_string();
        }

        let mut title_end = match_end;
        title_end.forward_word_end();

        // If go forward a word and we go past `{`, title is empty.
        if title_end.backward_search("{", search_flags, None).is_some() {
            return "".to_string();
        }

        let mut title_start = title_end;
        title_start.backward_word_start();

        // If we go back a word and it's `digraph`/`graph`, title is empty.
        if title_start == match_start {
            return "".to_string();
        }

        title_start.visible_text(&title_end).to_string()
    }

    #[allow(clippy::type_complexity)]
    async fn handle_file_io(
        &self,
        (io_fut, mut progress_stream): (
            impl Future<Output = Result<(), glib::Error>>,
            Pin<Box<dyn Stream<Item = (i64, i64)>>>,
        ),
    ) -> Result<()> {
        let progress_fut = async {
            while let Some((current_n_bytes, total_n_bytes)) = progress_stream.next().await {
                let progress = if total_n_bytes == 0 || current_n_bytes > total_n_bytes {
                    1.0
                } else {
                    current_n_bytes as f64 / total_n_bytes as f64
                };
                self.imp().busy_progress.set(progress);
                self.notify_busy_progress();
            }
        };

        let (io_ret, _) = join!(io_fut, progress_fut);
        io_ret?;

        Ok(())
    }

    fn update_style_scheme(&self) {
        let style_manager = adw::StyleManager::default();
        let style_scheme_manager = gtk_source::StyleSchemeManager::default();

        let style_scheme = if style_manager.is_dark() {
            style_scheme_manager
                .scheme("Adwaita-dark")
                .or_else(|| style_scheme_manager.scheme("classic-dark"))
        } else {
            style_scheme_manager
                .scheme("Adwaita")
                .or_else(|| style_scheme_manager.scheme("classic"))
        };

        self.set_style_scheme(style_scheme.as_ref());
    }
}
