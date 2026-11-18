use anyhow::{Context, Result};
use std::path::PathBuf;

const APP_DIR: &str = "gdcli";
const CLIENT_SECRET_FILE: &str = "client_secret.json";
const TOKEN_FILE: &str = "token.json";

#[derive(Debug, Clone)]
pub struct Paths {
    pub config: PathBuf,
    pub data: PathBuf,
}

fn base(variable: &str, fallback: &str) -> Option<PathBuf> {
    let absolute =
        |value: std::ffi::OsString| Some(PathBuf::from(value)).filter(|p| p.is_absolute());
    std::env::var_os(variable)
        .and_then(absolute)
        .or_else(|| {
            std::env::var_os("HOME")
                .and_then(absolute)
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
        Ok(Self::new(
            base("XDG_CONFIG_HOME", ".config")
                .context("cannot resolve XDG config home: set XDG_CONFIG_HOME or HOME")?,
            base("XDG_DATA_HOME", ".local/share")
                .context("cannot resolve XDG data home: set XDG_DATA_HOME or HOME")?,
        ))
    }

    pub fn client_secret(&self) -> PathBuf {
        self.config.join(CLIENT_SECRET_FILE)
    }

    pub fn token(&self) -> PathBuf {
        self.data.join(TOKEN_FILE)
    }
}
