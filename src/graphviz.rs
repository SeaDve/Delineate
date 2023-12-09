use std::path::Path;

use anyhow::{ensure, Error, Result};
use async_process::{Command, Stdio};
use futures_util::AsyncWriteExt;
use gettextrs::gettext;
use gtk::glib::{self, translate::TryFromGlib};

const PROGRAM: &str = "dot";

#[derive(Debug, Clone, Copy, glib::Enum)]
#[enum_type(name = "DaggerLayout")]
pub enum Layout {
    Dot,
    Neato,
    Twopi,
    Circo,
    Fdp,
    // Asage,
    Osage,
    Patchwork,
    // Sfdp,
}

impl TryFrom<i32> for Layout {
    type Error = i32;

    fn try_from(val: i32) -> Result<Self, Self::Error> {
        unsafe { Self::try_from_glib(val) }
    }
}

impl Layout {
    fn as_arg(&self) -> &'static str {
        match self {
            Self::Dot => "dot",
            Self::Neato => "neato",
            Self::Twopi => "twopi",
            Self::Circo => "circo",
            Self::Fdp => "fdp",
            Self::Osage => "osage",
            Self::Patchwork => "patchwork",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Format {
    Svg,
    Png,
    Webp,
    Pdf,
}

impl Format {
    pub fn extension(&self) -> &'static str {
        match self {
            Self::Svg => "svg",
            Self::Png => "png",
            Self::Webp => "webp",
            Self::Pdf => "pdf",
        }
    }

    pub fn mime_type(&self) -> &'static str {
        match self {
            Self::Svg => "image/svg+xml",
            Self::Png => "image/png",
            Self::Webp => "image/webp",
            Self::Pdf => "application/pdf",
        }
    }

    pub fn name(&self) -> String {
        match self {
            Self::Svg => gettext("SVG"),
            Self::Png => gettext("PNG"),
            Self::Webp => gettext("WebP"),
            Self::Pdf => gettext("PDF"),
        }
    }

    fn as_arg(&self) -> &'static str {
        match self {
            Self::Svg => "svg",
            Self::Png => "png",
            Self::Webp => "webp",
            Self::Pdf => "pdf",
        }
    }
}

/// Returns the version of the graphviz program.
pub async fn version() -> Result<String> {
    let output = Command::new(PROGRAM).arg("--version").output().await?;
    tracing::trace!(?output, "Child exited");

    ensure!(output.status.success(), "Failed to get version");

    Ok(String::from_utf8_lossy(&output.stderr)
        .trim_start_matches("dot - graphviz version ")
        .trim()
        .to_string())
}

/// Generates a graph from the given contents.
pub async fn generate(contents: &[u8], layout: Layout, format: Format) -> Result<Vec<u8>> {
    let mut child = dot_command(layout, format).spawn()?;

    child.stdin.take().unwrap().write_all(contents).await?;

    let output = child.output().await?;
    tracing::trace!(?output, "Child exited");

    if !output.status.success() {
        return Err(Error::msg(
            String::from_utf8_lossy(&output.stderr).to_string(),
        ));
    }

    Ok(output.stdout)
}

/// Exports the given contents to the given path.
pub async fn export(contents: &[u8], layout: Layout, format: Format, path: &Path) -> Result<()> {
    let mut child = dot_command(layout, format).arg("-o").arg(path).spawn()?;

    child.stdin.take().unwrap().write_all(contents).await?;

    let output = child.output().await?;
    tracing::trace!(?output, "Child exited");

    if !output.status.success() {
        return Err(Error::msg(
            String::from_utf8_lossy(&output.stderr).to_string(),
        ));
    }

    Ok(())
}

fn dot_command(layout: Layout, format: Format) -> Command {
    let mut command = Command::new(PROGRAM);

    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .args(["-T", format.as_arg()])
        .args(["-K", layout.as_arg()]);

    command
}
