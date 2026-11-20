mod fetch;

pub use fetch::{Fetcher, bearer, github, github_token};

use crate::google::{connector, https_builder};
use crate::output;
use anyhow::{Context, Result, bail, ensure};
use flate2::read::GzDecoder;
use http::StatusCode;
use http_body_util::BodyExt;
use semver::Version;
use serde_json::{Value, json};
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use tempfile::NamedTempFile;

const REPO: &str = "pycabbage/gdcli";
const API: &str = "https://api.github.com/repos/pycabbage/gdcli/releases/latest";
const LIMIT: u64 = 32 << 20;

pub fn version(release: &Value) -> Result<Version> {
    let tag = release["tag_name"].as_str().unwrap_or_default();
    let version = tag
        .strip_prefix('v')
        .and_then(|version| Version::parse(version).ok())
        .with_context(|| format!("latest release tag {tag:?} is not v<major>.<minor>.<patch>"))?;
    ensure!(
        version.pre.is_empty(),
        "latest release {tag} is a prerelease and is not installed"
    );
    Ok(version)
}

pub fn asset<'a>(release: &'a Value, platform: &str) -> Result<(&'a str, &'a str)> {
    let tag = release["tag_name"].as_str().unwrap_or_default();
    ensure!(
        release["immutable"] == true,
        "release {tag} is not immutable and is not installed"
    );
    let name = format!("gdcli_{platform}.tar.gz");
    let asset = release["assets"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|asset| asset["name"] == name.as_str())
        .with_context(|| format!("release {tag} has no asset {name}"))?;
    let digest = asset["digest"]
        .as_str()
        .and_then(|digest| digest.strip_prefix("sha256:"));
    let (Some(url), Some(sha256)) = (asset["browser_download_url"].as_str(), digest) else {
        bail!("asset {name} has no download url or sha256 digest");
    };
    Ok((url, sha256))
}

pub struct Install {
    pub dir: PathBuf,
    pub replace: fn(&Path) -> io::Result<()>,
    pub limit: u64,
}

fn extract(archive: File, dir: &Path, limit: u64) -> Result<NamedTempFile> {
    const EXTRACTION: &str = "cannot extract the archive";
    let mut archive = tar::Archive::new(GzDecoder::new(archive).take(4 * limit));
    for entry in archive.entries()? {
        let mut entry = entry.context(EXTRACTION)?;
        if entry.path_bytes().as_ref() == b"gdcli" && entry.header().entry_type().is_file() {
            ensure!(
                entry.size() <= limit,
                "archive entry gdcli is larger than {limit} bytes"
            );
            let mut staged = NamedTempFile::new_in(dir).context(EXTRACTION)?;
            io::copy(&mut entry, &mut staged).context(EXTRACTION)?;
            return Ok(staged);
        }
    }
    bail!("archive has no regular file named gdcli")
}

pub async fn run_with(
    fetcher: &Fetcher,
    api: &str,
    token: Option<&str>,
    install: &Install,
    out: &mut impl Write,
) -> Result<()> {
    let current = Version::parse(env!("CARGO_PKG_VERSION"))?;
    let header = token.map(bearer).transpose()?;
    let (response, sent) = fetcher.send(api, header.as_ref()).await?;
    ensure!(
        response.status() != StatusCode::NOT_FOUND,
        "github has no published release for {REPO}"
    );
    let body = Fetcher::ok(response, sent)?
        .into_body()
        .collect()
        .await
        .context("request failed")?
        .to_bytes();
    let release: Value = serde_json::from_slice(&body).context("unexpected github api response")?;
    let latest = version(&release)?;
    let status = if latest > current {
        let platform = format!("{}_{}", std::env::consts::OS, std::env::consts::ARCH);
        let (url, sha256) = asset(&release, &platform)?;
        let dir = &install.dir;
        let archive = tempfile::tempfile_in(dir)
            .with_context(|| format!("cannot write to {}", dir.display()))?;
        let archive = fetcher.archive(url, sha256, archive, install.limit).await?;
        let staged = extract(archive, dir, install.limit)?;
        (install.replace)(staged.path())
            .with_context(|| format!("cannot replace the executable in {}", dir.display()))?;
        "updated"
    } else {
        "up_to_date"
    };
    let record =
        json!({"status": status, "current": current.to_string(), "latest": latest.to_string()});
    Ok(output::record(out, &record, false)?)
}

pub async fn run(out: &mut impl Write) -> Result<()> {
    let dir = std::env::current_exe()
        .ok()
        .and_then(|exe| Some(exe.parent()?.to_owned()))
        .context("cannot locate the running executable")?;
    let fetcher = Fetcher {
        client: https_builder().build(connector()),
        allowed: github,
    };
    let install = Install {
        dir,
        replace: |staged| self_replace::self_replace(staged),
        limit: LIMIT,
    };
    let token = github_token(std::env::var_os("GITHUB_TOKEN"));
    run_with(&fetcher, API, token.as_deref(), &install, out).await
}
