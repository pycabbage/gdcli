use anyhow::{Context, Result, bail};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::Path;
use yup_oauth2::ConsoleApplicationSecret;

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
