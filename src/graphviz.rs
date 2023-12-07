use std::{io::Write, path::Path};

use anyhow::Result;
use async_process::Command;
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

/// Generate a PNG from the given DOT contents.
pub async fn run_with_str(contents: &str, layout: Layout) -> Result<Vec<u8>> {
    let mut input_file = NamedTempFile::new()?;
    input_file.write_all(contents.as_bytes())?;

    let input_path = input_file.into_temp_path();

    run(&input_path, layout).await
}

/// Generate a PNG from the given DOT file.
pub async fn run(input_path: &Path, layout: Layout) -> Result<Vec<u8>> {
    let output_path = NamedTempFile::new()?.into_temp_path();

    let format = "png";

    let child = Command::new("dot")
        .arg(input_path)
        .arg("-T")
        .arg(format)
        .arg("-K")
        .arg(layout.as_arg())
        .arg("-o")
        .arg(&output_path)
        .spawn()?;

    let output = child.output().await?;
    tracing::debug!(?output, "Child exited");

    let output_bytes = async_fs::read(&output_path).await?;

    Ok(output_bytes)
}
