use gettextrs::gettext;

#[derive(Debug, Clone, Copy)]
pub enum Format {
    Svg,
    Png,
    Jpeg,
}

impl Format {
    pub fn extension(&self) -> &'static str {
        match self {
            Self::Svg => "svg",
            Self::Png => "png",
            Self::Jpeg => "jpg",
        }
    }

    pub fn mime_type(&self) -> &'static str {
        match self {
            Self::Svg => "image/svg+xml",
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
        }
    }

    pub fn name(&self) -> String {
        match self {
            Self::Svg => gettext("SVG"),
            Self::Png => gettext("PNG"),
            Self::Jpeg => gettext("JPEG"),
        }
    }
}
