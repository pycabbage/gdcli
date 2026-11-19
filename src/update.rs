use crate::google::{Https, connector, https_builder};
use crate::output;
use anyhow::{Context, Result, bail, ensure};
use flate2::read::GzDecoder;
use http::header::{LOCATION, USER_AGENT};
use http::{Request, Response, StatusCode};
use http_body_util::BodyExt;
use hyper_util::client::legacy::Client;
use ring::digest;
use semver::Version;
use serde_json::{Value, json};
use std::fmt::Write as _;
use std::fs::File;
use std::io::{self, Read, Seek, Write};
use std::path::{Path, PathBuf};
use tempfile::NamedTempFile;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use url::Url;
use yup_oauth2::hyper::body::Incoming;

const REPO: &str = "pycabbage/gdcli";
const API: &str = "https://api.github.com/repos/pycabbage/gdcli/releases/latest";
const AGENT: &str = concat!("gdcli/", env!("CARGO_PKG_VERSION"));
const LIMIT: u64 = 32 << 20;
const REDIRECTS: usize = 3;

pub fn github(url: &Url) -> bool {
    let Some(host) = url.host_str() else {
        return false;
    };
    url.scheme() == "https"
        && url.authority() == host
        && (matches!(host, "github.com" | "api.github.com")
            || host.ends_with(".githubusercontent.com"))
}

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

pub struct Fetcher {
    pub client: Client<Https, String>,
    pub allowed: fn(&Url) -> bool,
}

impl Fetcher {
    async fn send(&self, url: &str) -> Result<Response<Incoming>> {
        let mut url = Url::parse(url).context("invalid download url")?;
        for _ in 0..=REDIRECTS {
            ensure!(
                (self.allowed)(&url),
                "refusing to fetch from unexpected host or scheme {}://{}",
                url.scheme(),
                url.host_str().unwrap_or_default()
            );
            let request = Request::get(url.as_str())
                .header(USER_AGENT, AGENT)
                .body(String::new())?;
            let response = self
                .client
                .request(request)
                .await
                .context("request failed")?;
            let next = response
                .headers()
                .get(LOCATION)
                .and_then(|location| url.join(location.to_str().ok()?).ok())
                .filter(|_| response.status().is_redirection());
            let Some(next) = next else {
                return Ok(response);
            };
            url = next;
        }
        bail!("too many redirects")
    }

    fn ok(response: Response<Incoming>) -> Result<Response<Incoming>> {
        let status = response.status();
        if status.is_success() {
            return Ok(response);
        }
        let header = |name| response.headers().get(name)?.to_str().ok();
        if matches!(
            status,
            StatusCode::FORBIDDEN | StatusCode::TOO_MANY_REQUESTS
        ) && header("x-ratelimit-remaining") == Some("0")
        {
            let reset = header("x-ratelimit-reset")
                .and_then(|seconds| OffsetDateTime::from_unix_timestamp(seconds.parse().ok()?).ok())
                .and_then(|time| time.format(&Rfc3339).ok());
            match reset {
                Some(time) => bail!("github api rate limit exceeded; resets at {time}"),
                None => bail!("github api rate limit exceeded"),
            }
        }
        bail!("github api error {status}")
    }

    pub async fn archive(&self, url: &str, want: &str, mut file: File, limit: u64) -> Result<File> {
        let mut body = Self::ok(self.send(url).await?)?.into_body();
        let mut hash = digest::Context::new(&digest::SHA256);
        let mut size = 0;
        while let Some(frame) = body.frame().await {
            if let Some(data) = frame.context("download failed")?.data_ref() {
                size += data.len() as u64;
                ensure!(size <= limit, "download exceeds {limit} bytes");
                hash.update(data);
                file.write_all(data).context("cannot write the download")?;
            }
        }
        let got = hash
            .finish()
            .as_ref()
            .iter()
            .fold(String::new(), |mut hex, byte| {
                let _ = write!(hex, "{byte:02x}");
                hex
            });
        ensure!(got == want, "checksum mismatch: expected {want}, got {got}");
        file.rewind()?;
        Ok(file)
    }
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
    install: &Install,
    out: &mut impl Write,
) -> Result<()> {
    let current = Version::parse(env!("CARGO_PKG_VERSION"))?;
    let response = fetcher.send(api).await?;
    ensure!(
        response.status() != StatusCode::NOT_FOUND,
        "github has no published release for {REPO}"
    );
    let body = Fetcher::ok(response)?
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
    run_with(&fetcher, API, &install, out).await
}
