# `gdcli update`

`gdcli update` replaces the running executable with the latest release of `pycabbage/gdcli` on GitHub. The output records and the error messages are specified in `commands.md`; this file records the design, the security level and the evidence. The code is `src/update.rs` (one module, 235 lines).

## Flow

1. `GET https://api.github.com/repos/pycabbage/gdcli/releases/latest` (one request, unauthenticated, `User-Agent: gdcli/<version>`).
2. The tag must be `v` plus a semver version without a prerelease part. If it is not greater than the running version (`CARGO_PKG_VERSION`) the command prints `status: up_to_date` and stops.
3. Otherwise the release must be immutable and contain the asset `gdcli_<OS>_<ARCH>.tar.gz` (`std::env::consts`, so `gdcli_linux_x86_64.tar.gz` and `gdcli_linux_aarch64.tar.gz`) with a download URL and a `sha256:` digest.
4. The first staging file is created in the install directory (the directory of `current_exe()`); a directory that is not writable fails here, before any download.
5. The asset is streamed into that unnamed file while its SHA-256 is computed; the digest must equal the one from the API before the archive is opened.
6. The tar.gz is read once: the entry named exactly `gdcli` that is a regular file is copied into a second staging file (`NamedTempFile` in the install directory).
7. `self_replace::self_replace` swaps the executable; the command prints `status: updated`.

## Version and release rules

- The tag is `v<major>.<minor>.<patch>`; anything else, and any prerelease part, is an error. A semver compare (`semver` crate) decides; `latest <= current` is up to date, so there is no downgrade and no reinstall.
- GitHub orders `releases/latest` by the creation time of the release (`created_at`), not by semver. Publish versions in increasing order; a backport published after a newer release makes `latest` smaller than the running version, which reports `up_to_date`.
- `.goreleaser.yaml` has `prerelease: auto` under `release:`, so GoReleaser marks a release whose tag has a semver prerelease suffix (`v1.0.0-rc1`) as a GitHub prerelease, and `releases/latest` excludes prereleases. Evidence (GoReleaser v2.20.2): `internal/pipe/release/release.go` lines 75-84 (`case "auto": if ctx.Semver.Prerelease != "" { ctx.PreRelease = true }`), `internal/client/github.go` line 544 (the `prerelease` and `make_latest` fields of the release request), and a run of GoReleaser against a mock GitHub API (scratch, not in the repository): tag `v1.0.0-rc1` sent `POST releases` with `draft: true, prerelease: true` and then `PATCH` with `draft: false, prerelease: true, make_latest: "false"`; tag `v1.0.0` sent `prerelease: false`. `goreleaser check` accepts the key. No prerelease tag was published (an immutable tag cannot be reused), so the first real one is the live test. The prerelease refusal in the code stays as a backstop.
- The asset must exist only when an update is due, so a missing asset on an old release is not an error. The asset name contains the OS, so a macOS or Windows build of gdcli fails with `release <tag> has no asset gdcli_macos_aarch64.tar.gz` instead of replacing its executable with a Linux binary.
- `immutable: true` is required. This is hardening, not the basis of the integrity claim (that is the digest): it fails closed if GitHub ever stops reporting the flag or a release was published mutable. If the repository setting is disabled, updates fail with an explicit message and users reinstall as described in the README.
- The `gdcli_checksums.txt` asset is not used: it has the same origin and trust root as the API digest and would cost a second download and a parser; GitHub has computed digests for every asset of this repository.

## Allowed hosts

`github(url)` accepts a URL only if the scheme is `https`, the authority is exactly the host (no userinfo, no non-default port) and the host is `github.com`, `api.github.com` or ends with `.githubusercontent.com`. Every request goes through it: the constant API URL, the `browser_download_url` from the JSON and every redirect target. At most 3 redirects are followed. The real download is `github.com/.../releases/download/...`, one 302 to `release-assets.githubusercontent.com`, then 200. A rejected URL is reported as `scheme://host` only (no userinfo, port, path, query or signed parameters).

The predicate is a separate function and not a shared helper with `google::googleusercontent`: the Google predicate is 6 lines with its own tests; a shared `https_host(url, accepted)` would be about 6 lines, shrink the Google predicate to 3 and the GitHub one to 6, i.e. 15 lines against 6 + 10, one line saved at the price of editing a tested Google path whose port and userinfo behaviour would change or need a parameter.

## Security level

- Integrity, not authenticity. The digest is computed by GitHub and delivered by `api.github.com`; the bytes come from a different host. That stops corruption, truncation and tampering on the download path (CDN, storage, a middlebox that does not also control the API host). Immutable releases prevent changing a published release afterwards.
- Not defended: a compromised GitHub account, token or workflow that publishes a malicious new release with a matching digest, and a compromised root CA in `webpki-roots`.
- `gh attestation verify` is not performed by the program (the build provenance attestation is described in `release.md`; users can verify an asset by hand). Verifying Sigstore bundles from Rust needs `sigstore-verify` (+34 packages including `aws-lc-sys`, a C library); shelling out to `gh` needs a tool most users lack.
- No credentials: `src/update.rs` imports neither `auth` nor `Google`, builds no `Authorization` header and never reads the config or data directories; `grep -nE "Authorization|crate::auth|Google|token" src/update.rs` prints nothing. Token support (a higher rate limit) is not provided: it would put a credential on a path that follows redirects, and one request per run is far below the 60 requests per hour per IP limit.
- Archive handling: the tar entry name is only compared with `gdcli`, never joined to a path; `Entry::unpack` is never called, so names such as `../x`, absolute paths, symlinks and hard links cannot touch the file system, and every other entry is skipped. A `gdcli` entry that is a symlink or a directory is skipped and ends in `archive has no regular file named gdcli`. RUSTSEC-2026-0067 concerns `unpack_in`, which is never called; RUSTSEC-2026-0068 concerns PAX size-header parsing, which `Archive::entries()` uses, so `tar >= 0.4.45` is required.
- Same-user local attackers are out of scope: the staged file is 0600 with a random name, and such an attacker can replace the binary directly.

## Staging, replacement and ETXTBSY

- Staging happens in the install directory, never in `TMPDIR`: the directory must be writable for the replacement anyway, nothing is staged in tmpfs (RAM), and a non-writable directory fails before the download with `cannot write to <dir>`. Disk use there is about 9 MB during an update (archive 1.8 MB, staged binary 3.7 MB, `self-replace`'s copy 3.7 MB).
- Opening a running executable for writing fails with ETXTBSY; `rename(2)` replaces the directory entry and the running process keeps the old inode. `self-replace` therefore copies the staged file to `.gdcli.__temp__XXXXXX` next to the executable, gives it the permissions of the installed file and renames it over the executable. The update runs from the very file it replaces.
- `current_exe()` is `/proc/self/exe` on Linux, already symlink-resolved: a symlinked `~/.local/bin/gdcli` replaces its target and the link survives.
- Failure atomicity: the only change to the install location is that final rename. The unnamed archive file, the `NamedTempFile` and `self-replace`'s temp file are drop guards, so every error path and the deadline remove them. `SIGKILL`, power loss or Ctrl-C at the wrong moment can leave `.tmpXXXXXX` (ours) or `.gdcli.__temp__XXXXXX` (`self-replace`) in the install directory; both are safe to delete. The installed binary is never partially written.
- `cargo run -- update` would replace `target/debug/gdcli` with the release binary; harmless, cargo rebuilds it.
- A package-manager-owned file (for example in `/usr/bin`) is replaced only if the user can write to its directory; otherwise the command fails early with the path in the message.

## Bounds

- Size: `limit` is 32 MiB (real sizes: 1.8 MB archive, 3.7 MB binary). It bounds the download, the `gdcli` entry (checked with `entry.size()`; `take` would truncate silently) and, times four, the whole inflated stream (`Read::take` on the gzip decoder). Together they bound disk, memory and total inflated bytes against an endpoint that never stops, an oversized entry and a decompression bomb inside a verified archive. An inflated stream cut in the middle of an entry fails with `cannot extract the archive: unexpected EOF`; cut exactly at an entry boundary the tar reader sees a clean end and the error is `archive has no regular file named gdcli`; either way nothing is installed.
- Time: every network wait is bounded by the connect timeout (10 s) and the 120 s deadline of `with_deadline` (`deadline_for` gives every command except `setup` and `login` that deadline); the deadline drops the future, which drops the staging files. Extraction and replacement are synchronous and the deadline cannot interrupt them; their cost is bounded by the size limits and local disk speed, milliseconds for the real binary.
- Memory: frames go from hyper to the hash and the file; the only buffered body is the API JSON (about 10 KB). Peak RSS of a real update is recorded in `performance.md`.
- Rate limit: 60 requests per hour per IP for unauthenticated API calls (`x-ratelimit-limit`, `x-ratelimit-remaining`, `x-ratelimit-reset`); one run costs one request (the asset download is not a REST call). A 403 or 429 with `x-ratelimit-remaining: 0` becomes the rate-limit error with the reset time. Secondary (abuse) limits answer 403 or 429 with `retry-after` and without `x-ratelimit-remaining: 0`; they are reported as `github api error 403 Forbidden` and not handled specially.

## Injection and tests

Production values are written only in `run`: the client from `google::https_builder()` and `google::connector()`, the predicate `github`, the API URL constant, the directory of `current_exe()`, `self_replace::self_replace` and the 32 MiB limit. `run_with(&Fetcher, api, &Install, out)` takes everything that differs in tests, the same way `Google::new` and `Paths::resolve` do, so no environment variable, flag or Cargo feature can change where a binary comes from. The 22 tests of `tests/update.rs` run `run_with` in-process against wiremock with a plain-HTTP client, the predicate `github(url) || host == 127.0.0.1`, a fake replace function that copies the staged file to `<dir>/installed`, and small limits. What they cannot cover is the real replacement of a running executable and the production wiring of `run`; that is proven by the real runs against the real release (see `testing.md`), which are a release gate.

Rejected test seams: an environment variable in the release binary (anything that can set the environment of one `gdcli update` decides where the executable comes from), `#[cfg(debug_assertions)]` (ties security code to a profile switch; `cargo test --release` would silently contact GitHub) and a Cargo feature enabled for tests through a self dev-dependency (it works, but it adds a lock self edge and two gates for a seam that plain injection makes unnecessary).

## What stays unverified

Execution of the aarch64 binary and of an aarch64 update (no aarch64 host; the build is checked with `cargo zigbuild --target aarch64-unknown-linux-gnu`), a root-owned or package-managed install path (covered by the read-only directory run only), and the first real prerelease tag.

## Alternatives rejected

| Alternative | Measured or known cost | Decision |
|---|---|---|
| `self_update` crate | +2,451,448 bytes (+67 %), +39 packages, a second HTTP client (`ureq`), mandatory `regex`, ignores draft, prerelease and immutable, walks all release pages, a fresh agent-assisted 1.x rewrite | rejected |
| `ubi` | +3.45 MB, +86 packages, a `chrono` unmaintained advisory (RUSTSEC-2026-0543) in its tree, in-place copy fails with ETXTBSY | rejected |
| `axoupdater` | needs `dist` install receipts and an installer asset this project does not publish | rejected |
| `binstalk` | +94 packages | rejected |
| Sigstore verification (`sigstore-verify`) | +34 packages including `aws-lc-sys` (C) | rejected, integrity only |
| `sha2` instead of `ring::digest` | `digest`, `block-buffer`, `crypto-common`, `generic-array`, `cpufeatures` and a second SHA-256 | rejected: `ring` is already in the tree |
| manual `(u64, u64, u64)` version compare | about 8 lines plus edge-case tests; accepts `v1.2.3.4` unless written carefully | rejected: `semver` |
| hand-written replace | 12 lines of temp file, copy, chmod and rename | rejected: `self-replace` (48 lines, used by uv, pixi, mise, axoupdater) |
| `miniz_oxide` backend of `flate2` | 3 more packages, slower inflate, 12.5 KB smaller binary | rejected: `zlib-rs` |
| checksum file fallback | same trust root, second download and parser | rejected |
| `--check`, `--json`, GitHub token | extra behaviour, tests and docs; a credential on a redirecting path | rejected; `--json` exists only where the owner listed it |
| `CARGO_PKG_REPOSITORY` for the coordinates | needs a `repository` field and runtime parsing | rejected: two constants |
