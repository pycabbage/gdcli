use super::token::{authenticator, secret};
use crate::google::{ApiError, Google, connector, https_builder};
use crate::paths::Paths;
use anyhow::{Context, Result, anyhow};
use serde_json::Value;
use std::fmt;
use std::fs::{self, DirBuilder, Permissions};
use std::io::{ErrorKind, Write};
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::PathBuf;
use std::str::FromStr;
use tempfile::NamedTempFile;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Email(String);

impl FromStr for Email {
    type Err = String;

    fn from_str(raw: &str) -> Result<Self, String> {
        let email = raw.trim().to_ascii_lowercase();
        let part = |text: &str, extra: &str| {
            !text.is_empty()
                && text
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || extra.contains(c))
        };
        let valid = email.len() <= 128
            && !email.starts_with('.')
            && !email.contains("..")
            && email
                .split_once('@')
                .is_some_and(|(local, domain)| part(local, "._+-") && part(domain, ".-"));
        if valid {
            Ok(Self(email))
        } else {
            Err("expected an email address such as name@example.com".to_owned())
        }
    }
}

impl fmt::Display for Email {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.0)
    }
}

pub struct Stored {
    pub email: Email,
    pub added: bool,
    pub default: bool,
}

pub fn create(paths: &Paths) -> Result<()> {
    DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(paths.accounts())
        .with_context(|| format!("cannot create {}", paths.accounts().display()))
}

pub fn list(paths: &Paths) -> Result<Vec<Email>> {
    let entries = match fs::read_dir(paths.accounts()) {
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        entries => {
            entries.with_context(|| format!("cannot read {}", paths.accounts().display()))?
        }
    };
    let mut accounts: Vec<Email> = entries
        .filter_map(|entry| {
            let name = entry.ok()?.file_name().into_string().ok()?;
            let email: Email = name.strip_suffix(".json")?.parse().ok()?;
            (format!("{email}.json") == name).then_some(email)
        })
        .filter(|email| paths.account(email).is_file())
        .collect();
    accounts.sort();
    Ok(accounts)
}

fn absent(paths: &Paths, message: &str) -> anyhow::Error {
    match list(paths) {
        Ok(accounts) if accounts.is_empty() => {
            anyhow!("no account is logged in: run `gdcli auth login`")
        }
        Ok(accounts) => {
            let known: Vec<String> = accounts.iter().map(Email::to_string).collect();
            anyhow!("{message}; known accounts: {}", known.join(", "))
        }
        Err(error) => error,
    }
}

pub fn default_account(paths: &Paths) -> Result<Email> {
    let file = paths.default_account();
    match fs::read_to_string(&file) {
        Ok(text) => text.parse::<Email>().map_err(|error| {
            anyhow!(
                "invalid default account in {}: {error}; run `gdcli auth set-default <email>`",
                file.display()
            )
        }),
        Err(error) if error.kind() == ErrorKind::NotFound => match list(paths)?.as_slice() {
            [only] => Ok(only.clone()),
            _ => Err(absent(
                paths,
                "no default account; run `gdcli auth set-default <email>`",
            )),
        },
        Err(error) => Err(error).with_context(|| format!("cannot read {}", file.display())),
    }
}

pub fn token_file(paths: &Paths, requested: Option<&Email>) -> Result<(Email, PathBuf)> {
    let email = match requested {
        Some(email) => email.clone(),
        None => default_account(paths)?,
    };
    let file = paths.account(&email);
    if file.is_file() {
        return Ok((email, file));
    }
    Err(absent(
        paths,
        &if requested.is_some() {
            format!("unknown account {email}")
        } else {
            format!(
                "default account {email} has no token: run `gdcli auth login` or `gdcli auth set-default <email>`"
            )
        },
    ))
}

fn write_default(paths: &Paths, email: &Email) -> Result<()> {
    let write = || -> Result<()> {
        let mut file = NamedTempFile::new_in(&paths.data)?;
        writeln!(file, "{email}")?;
        file.persist(paths.default_account())?;
        Ok(())
    };
    write().with_context(|| format!("cannot write {}", paths.default_account().display()))
}

pub fn set_default(paths: &Paths, email: &Email) -> Result<()> {
    if !paths.account(email).is_file() {
        return Err(absent(paths, &format!("unknown account {email}")));
    }
    write_default(paths, email)
}

pub async fn email(google: &Google) -> Result<Email> {
    let query = [("fields", "user(emailAddress)")];
    let discover = async {
        let about = google.json(&google.drive, &["about"], &query).await?;
        about
            .pointer("/user/emailAddress")
            .and_then(Value::as_str)
            .context("google did not report an email address for this account")?
            .parse::<Email>()
            .map_err(|_| anyhow!("unsupported email address"))
    };
    discover.await.map_err(|error| {
        let drive_off = error
            .chain()
            .filter_map(|cause| cause.downcast_ref::<ApiError>())
            .any(|api| matches!(api.status.as_u16(), 403 | 404));
        let hint = if drive_off {
            " (enable the Google Drive API for the project)"
        } else {
            ""
        };
        anyhow!("cannot identify the account: {error:#}{hint}")
    })
}

pub async fn store(paths: &Paths, pending: NamedTempFile, google: &Google) -> Result<Stored> {
    let email = email(google).await?;
    let first = list(paths)?.is_empty();
    let file = paths.account(&email);
    let added = !file.exists();
    pending
        .persist(&file)
        .with_context(|| format!("cannot write {}", file.display()))?;
    if first {
        write_default(paths, &email)?;
    }
    let default = default_account(paths).is_ok_and(|default| default == email);
    Ok(Stored {
        email,
        added,
        default,
    })
}

pub async fn adopt(paths: &Paths, google: &Google) -> Result<()> {
    let email = email(google).await?;
    create(paths)?;
    write_default(paths, &email)?;
    let (legacy, target) = (paths.legacy_token(), paths.account(&email));
    match fs::rename(&legacy, &target) {
        Err(error) if error.kind() == ErrorKind::NotFound && target.exists() => {}
        moved => moved
            .with_context(|| format!("cannot move {} to {}", legacy.display(), target.display()))?,
    }
    fs::set_permissions(&target, Permissions::from_mode(0o600))
        .with_context(|| format!("cannot restrict permissions of {}", target.display()))
}

pub async fn migrate(paths: &Paths) -> Result<()> {
    let legacy = paths.legacy_token();
    if !legacy.exists() || !list(paths)?.is_empty() {
        return Ok(());
    }
    let secret = secret(paths).await?;
    let client = https_builder().build(connector());
    let auth = authenticator(secret, &legacy, None, None, client.clone()).await?;
    adopt(paths, &Google::production(client, auth)).await
}
