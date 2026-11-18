use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub const APP_DIR: &str = "gdcli";
pub const CLIENT_SECRET_FILE: &str = "client_secret.json";
pub const TOKEN_FILE: &str = "token.json";

#[derive(Debug, Clone)]
pub struct Paths {
    config: PathBuf,
    data: PathBuf,
}

impl Paths {
    pub fn new(config: impl Into<PathBuf>, data: impl Into<PathBuf>) -> Self {
        Self {
            config: config.into(),
            data: data.into(),
        }
    }

    pub fn from_xdg() -> Result<Self> {
        let dirs = xdg::BaseDirectories::with_prefix(APP_DIR);
        Ok(Self::new(
            dirs.get_config_home()
                .context("cannot resolve XDG config home: set XDG_CONFIG_HOME or HOME")?,
            dirs.get_data_home()
                .context("cannot resolve XDG data home: set XDG_DATA_HOME or HOME")?,
        ))
    }

    pub fn config_dir(&self) -> &Path {
        &self.config
    }

    pub fn data_dir(&self) -> &Path {
        &self.data
    }

    pub fn client_secret(&self) -> PathBuf {
        self.config.join(CLIENT_SECRET_FILE)
    }

    pub fn token(&self) -> PathBuf {
        self.data.join(TOKEN_FILE)
    }
}
