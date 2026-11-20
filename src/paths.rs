use crate::auth::accounts::Email;
use anyhow::{Context, Result};
use std::ffi::OsString;
use std::path::PathBuf;

const APP_DIR: &str = "gdcli";
const CLIENT_SECRET_FILE: &str = "client_secret.json";
const ACCOUNTS_DIR: &str = "accounts";
const DEFAULT_FILE: &str = "default";
const LEGACY_TOKEN_FILE: &str = "token.json";

#[derive(Debug, Clone)]
pub struct Paths {
    pub config: PathBuf,
    pub data: PathBuf,
}

pub type Env<'a> = &'a dyn Fn(&str) -> Option<OsString>;

fn base(
    env: Env,
    home: &dyn Fn() -> Option<PathBuf>,
    variable: &str,
    fallback: &str,
) -> Option<PathBuf> {
    let absolute = |value: OsString| Some(PathBuf::from(value)).filter(|p| p.is_absolute());
    env(variable)
        .and_then(absolute)
        .or_else(|| {
            env("HOME")
                .and_then(absolute)
                .or_else(home)
                .map(|home| home.join(fallback))
        })
        .map(|path| path.join(APP_DIR))
}

impl Paths {
    pub fn new(config: impl Into<PathBuf>, data: impl Into<PathBuf>) -> Self {
        Self {
            config: config.into(),
            data: data.into(),
        }
    }

    pub fn from_xdg() -> Result<Self> {
        Self::resolve(&|name| std::env::var_os(name), &std::env::home_dir)
    }

    pub fn resolve(env: Env, home: &dyn Fn() -> Option<PathBuf>) -> Result<Self> {
        Ok(Self::new(
            base(env, home, "XDG_CONFIG_HOME", ".config")
                .context("cannot resolve XDG config home: set XDG_CONFIG_HOME or HOME")?,
            base(env, home, "XDG_DATA_HOME", ".local/share")
                .context("cannot resolve XDG data home: set XDG_DATA_HOME or HOME")?,
        ))
    }

    pub fn client_secret(&self) -> PathBuf {
        self.config.join(CLIENT_SECRET_FILE)
    }

    pub fn accounts(&self) -> PathBuf {
        self.data.join(ACCOUNTS_DIR)
    }

    pub fn account(&self, email: &Email) -> PathBuf {
        self.accounts().join(format!("{email}.json"))
    }

    pub fn default_account(&self) -> PathBuf {
        self.data.join(DEFAULT_FILE)
    }

    pub fn legacy_token(&self) -> PathBuf {
        self.data.join(LEGACY_TOKEN_FILE)
    }
}
