use anyhow::Result;
use async_process::{Command, Stdio};
use futures_util::AsyncWriteExt;
use gtk::glib::{self, translate::TryFromGlib};
use tempfile::NamedTempFile;

#[derive(Debug, Clone, Copy, glib::Enum)]
#[enum_type(name = "DaggerLayout")]
pub enum Layout {
    Dot,
    Neato,
    Twopi,
    Circo,
    Fdp,
    // Asage,
    Patchwork,
    Sfdp,
}

impl TryFrom<i32> for Layout {
    type Error = i32;

    fn try_from(val: i32) -> Result<Self, Self::Error> {
        unsafe { Self::try_from_glib(val) }
    }
}

impl Layout {
    fn as_arg(self) -> &'static str {
        match self {
            Self::Dot => "dot",
            Self::Neato => "neato",
            Self::Twopi => "twopi",
            Self::Circo => "circo",
            Self::Fdp => "fdp",
            Self::Patchwork => "patchwork",
            Self::Sfdp => "sfdp",
        }
    }
}

pub enum Format {
    Svg,
    Png,
}

impl Format {
    fn as_arg(self) -> &'static str {
        match self {
            Self::Svg => "svg",
            Self::Png => "png",
        }
    }
}

/// Generate a PNG from the given DOT contents.
pub async fn run(contents: &[u8], layout: Layout, format: Format) -> Result<Vec<u8>> {
    let output_path = NamedTempFile::new()?.into_temp_path();

    let mut child = Command::new("dot")
        .stdin(Stdio::piped())
        .arg("-T")
        .arg(format.as_arg())
        .arg("-K")
        .arg(layout.as_arg())
        .arg("-o")
        .arg(&output_path)
        .spawn()?;

    child.stdin.take().unwrap().write_all(contents).await?;

    let output = child.output().await?;
    tracing::debug!(?output, "Child exited");

    let output_bytes = async_fs::read(&output_path).await?;

    Ok(output_bytes)
}
