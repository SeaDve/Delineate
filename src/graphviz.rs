use anyhow::{ensure, Error, Result};
use async_process::{Command, Stdio};
use futures_util::AsyncWriteExt;
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
}

impl Format {
    fn as_arg(&self) -> &'static str {
        match self {
            Self::Svg => "svg",
            Self::Png => "png",
        }
    }
}

pub async fn version() -> Result<String> {
    let output = Command::new(PROGRAM).arg("--version").output().await?;

    ensure!(output.status.success(), "Failed to get version");

    Ok(String::from_utf8_lossy(&output.stderr)
        .trim_start_matches("dot - graphviz version ")
        .trim()
        .to_string())
}

/// Generate a PNG from the given DOT contents.
pub async fn run(contents: &[u8], layout: Layout, format: Format) -> Result<Vec<u8>> {
    let mut child = Command::new(PROGRAM)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .arg("-T")
        .arg(format.as_arg())
        .arg("-K")
        .arg(layout.as_arg())
        .spawn()?;

    child.stdin.take().unwrap().write_all(contents).await?;

    let output = child.output().await?;
    tracing::trace!(?output, "Child exited");

    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err(Error::msg(
            String::from_utf8_lossy(&output.stderr).to_string(),
        ))
    }
}
