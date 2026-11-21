use super::login;
use crate::paths::Paths;
use anyhow::{Context, Result, bail};
use std::fs::{self, OpenOptions};
use std::io::{BufRead, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use yup_oauth2::ConsoleApplicationSecret;
use yup_oauth2::authenticator_delegate::InstalledFlowDelegate;

const INSTRUCTIONS: &str = include_str!("../../docs/setup.md");

pub fn typed_path(answer: &str, home: Option<&Path>) -> PathBuf {
    let path = answer.trim().trim_matches(['"', '\'']).trim();
    match (path.strip_prefix("~/"), home) {
        (Some(rest), Some(home)) => home.join(rest),
        _ => PathBuf::from(path),
    }
}

pub async fn setup(
    paths: &Paths,
    mut input: impl BufRead,
    out: &mut impl Write,
    flow: impl InstalledFlowDelegate + 'static,
) -> Result<()> {
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
    if source.as_os_str().is_empty() {
        let json = std::fs::read(&destination)
            .with_context(|| format!("cannot read {}", destination.display()))?;
        validate_client_secret(&json)
            .with_context(|| format!("invalid file {}", destination.display()))?;
    } else {
        install_client_secret(&source, &destination)?;
    }
    login(paths, out, flow).await
}

pub fn validate_client_secret(json: &[u8]) -> Result<()> {
    let secret: ConsoleApplicationSecret =
        serde_json::from_slice(json).context("not a valid OAuth client secret JSON")?;
    match secret.installed {
        Some(_) => Ok(()),
        None => bail!(
            "client secret has no \"installed\" section: create the OAuth client with application type \"Desktop app\""
        ),
    }
}

pub fn install_client_secret(source: &Path, destination: &Path) -> Result<()> {
    let json = fs::read(source).with_context(|| format!("cannot read {}", source.display()))?;
    validate_client_secret(&json).with_context(|| format!("invalid file {}", source.display()))?;
    if let Some(parent) = destination.parent() {
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(parent)
            .with_context(|| format!("cannot create {}", parent.display()))?;
    }
    OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(destination)
        .and_then(|mut file| file.write_all(&json))
        .with_context(|| format!("cannot write {}", destination.display()))?;
    fs::set_permissions(destination, fs::Permissions::from_mode(0o600))
        .with_context(|| format!("cannot restrict permissions of {}", destination.display()))
}
