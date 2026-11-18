use super::{install_client_secret, login, validate_client_secret};
use crate::paths::Paths;
use anyhow::{Context, Result};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

const INSTRUCTIONS: &str = include_str!("../../docs/setup.md");

pub fn typed_path(answer: &str, home: Option<&Path>) -> PathBuf {
    let path = answer.trim().trim_matches(['"', '\'']).trim();
    match (path.strip_prefix("~/"), home) {
        (Some(rest), Some(home)) => home.join(rest),
        _ => PathBuf::from(path),
    }
}

pub async fn setup(paths: &Paths, mut input: impl BufRead, out: &mut impl Write) -> Result<()> {
    let destination = paths.client_secret();
    writeln!(out, "{INSTRUCTIONS}")?;
    writeln!(out, "Destination: {}", destination.display())?;
    write!(
        out,
        "Path of the downloaded JSON (empty if already placed at the destination): "
    )?;
    out.flush()?;
    let mut answer = String::new();
    input.read_line(&mut answer)?;
    let source = typed_path(&answer, std::env::home_dir().as_deref());
    match source.as_os_str().is_empty() {
        true => {
            let json = std::fs::read(&destination)
                .with_context(|| format!("cannot read {}", destination.display()))?;
            validate_client_secret(&json)
                .with_context(|| format!("invalid file {}", destination.display()))?;
        }
        false => install_client_secret(&source, &destination)?,
    }
    login(paths, out).await
}
