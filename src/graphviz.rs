use std::{io::Write, path::Path};

use anyhow::Result;
use async_process::Command;
use tempfile::NamedTempFile;

/// Generate a PNG from the given DOT contents.
pub async fn run_with_str(contents: &str) -> Result<Vec<u8>> {
    let mut in_file = NamedTempFile::new()?;
    in_file.write_all(contents.as_bytes())?;

    let in_path = in_file.into_temp_path();

    run(&in_path).await
}

/// Generate a PNG from the given DOT file.
pub async fn run(in_path: &Path) -> Result<Vec<u8>> {
    let out_path = NamedTempFile::new()?.into_temp_path();

    let child = Command::new("dot")
        .arg("-T")
        .arg("png")
        .arg(in_path)
        .arg("-o")
        .arg(&out_path)
        .spawn()?;

    let output = child.output().await?;
    tracing::debug!(?output, "Child exited");

    let out_bytes = async_fs::read(&out_path).await?;

    Ok(out_bytes)
}
