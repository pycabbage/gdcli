# Performance and memory

Priorities: startup time, resident memory, bytes transferred, number of round trips.

## Build

`[profile.release]`: `opt-level = 3`, `lto = "fat"`, `codegen-units = 1`, `panic = "abort"`, `strip = true`. The binary links only libc and libgcc dynamically (no OpenSSL; certificates are bundled through `webpki-roots`), so it cross-compiles with `zig cc` for x86_64 and aarch64 `linux-gnu`.

## Runtime decisions

- tokio `current_thread` runtime with only the `rt`, `macros` and `time` features (`time` for the overall deadline); no worker threads.
- Independent requests are concurrent with `tokio::try_join!` on one shared hyper client: past and upcoming lookups in `calendar list` and `meet list` (both Calendar only); space lookup and conference list in `meet get`, then participants, recordings and transcripts of each conference record.
- The production client is HTTP/2-only (`http2_only(true)` on the hyper-util builder, ALPN `h2` on the rustls connector; every Google endpoint speaks h2), so concurrent requests are multiplexed over one TCP+TLS connection instead of each opening its own. Tests build their own protocol-negotiating client with a plain-HTTP connector for wiremock; production code uses `google::https_builder()` with the https-only connector (plain HTTP can never be reached in production). The same builder serves the OAuth token client.
- Timeouts (constants in `src/google.rs`): 10 s TCP connect timeout on the connector, and a 120 s deadline around every API command (`with_deadline`; `setup` and `login` wait for the user and are exempt), failing with `error: timed out after 120s`.
- Every JSON request sends the `fields` partial-response parameter (the Markdown export has none); responses contain only printed fields. Responses are handled as `serde_json::Value`, with no typed models and no generated crates.
- `calendar list` upcoming requests `maxResults=5`; past windows keep a 5 element ring buffer instead of collecting a window.
- `docs get` writes each body frame to stdout as it arrives; the document is never held in memory. Only the error path buffers (a small JSON error body).
- Rendering writes directly into the caller's `impl Write`.
- Dependencies are pulled with default features off (clap, tokio, hyper-rustls, hyper-util, time, yup-oauth2). Non-auth commands never open a listening socket and never start a browser flow.
- Directories are created only when something is written (`login` creates the data directory; no other command creates any).
- `calendar list` and `meet list` send the `eventTypes` filter (default, focusTime, outOfOffice, fromGmail), so working-location and birthday entries are neither transferred nor parsed.

## Measurements (x86_64 Linux, release profile, rustc 1.100)

| Measurement | Before the REST layer (generated Google crates, chrono) | After the REST layer | Now (JSON, tabs, metadata, counts) | With `gdcli update` |
|---|---|---|---|---|
| Binary size (stripped) | 4,334,864 bytes | 3,565,016 bytes | 3,661,096 bytes | 3,812,312 bytes (+151,216, +4.13 %; aarch64 `cargo zigbuild`: 3,264,784 bytes); commit and `v0.2.0` figure: see the size bullet under `gdcli update` |
| Unique packages in the normal dependency tree, including gdcli (`cargo tree -e normal --prefix none --no-dedupe \| sed 's/ (.*//' \| sort -u \| wc -l`) | 123 | 108 | 109 (`foldhash` through `preserve_order`) | 121 (`self-replace`, `tempfile`, `tar`, `flate2`, `semver`, `zlib-rs`, `fastrand`, `rustix`, `linux-raw-sys`, `bitflags`, `filetime`, `getrandom` 0.4.3) |
| Peak RSS of `gdcli --help` (`/usr/bin/time -f %M`, 3 runs) | 3576, 3576, 3632 KB | 3468, 3388, 3376 KB | 3536, 3700, 3572 KB | 3848, 3760, 3784 KB |
| Peak RSS of a command with a real HTTPS round trip to Google and an invalid token (so the answer is a small 401) | 6.0 to 6.3 MB | 5.6 to 5.8 MB | `calendar list` 5728, `meet list` 5888, `docs get --tab` 5600, `docs metadata` 5888 KB | not re-measured (no change to the Google path); `gdcli update` against the real release: 6116 KB in the final run (6108 to 6288 KB over four runs; staging in the install directory, no tmpfs); 6192 KB (`/usr/bin/time -f %M`) when the self-update script of `release.md` updated a scratch 0.0.1 build to the genuine `v0.2.0` on 2026-11-19 |
| TCP connections to port 443 per command (`strace -f -e trace=connect -o out gdcli <command>`, then `grep -c 'sin_port=htons(443)\|sin6_port=htons(443)' out`; invalid-token setup) | `calendar list` 2, `meet get` 4 (before HTTP/2-only) | 1 for every command | 1 for every command except `docs metadata`: 2, because Drive (`www.googleapis.com`) and the Docs API (`docs.googleapis.com`) are two hosts, one HTTP/2 connection each, run concurrently | not measured for `update` (it talks to `api.github.com`, `github.com` and `release-assets.githubusercontent.com`, three hosts, HTTP/2 only) |
| Source lines, `src/` (non-blank, after `cargo fmt`) | 1282 | 929 | 1324 (log in `code-reduction.md`) | 1551 (`src/update.rs` 223 non-blank, 235 with blank lines; log in `code-reduction.md`) |

With an invalid token `meet get` and `docs get --tab` stop after the first request, so their connection counts for the full flows (events, space, records, artifacts; tab list, export and the redirect to the content host) have to be measured with a valid token. The tab export touches three hosts by design: `docs.googleapis.com` (tab list), `docs.google.com` (export) and the `googleusercontent.com` content host (the redirect target, fetched without the token).

Measuring a successful call needs real credentials and was not done here; the streaming and ring-buffer designs bound the memory of the success paths independent of document or calendar size. The `--past` / `--upcoming` counts bound the memory of the lists by N.

## `gdcli update`

- One HTTP/2 client from `google::https_builder()`; the archive is streamed from hyper into the hash and an unnamed file in the install directory (no whole-archive buffer, no tmpfs staging); only the API JSON (about 10 KB) is buffered. The extracted binary is copied once with `io::copy` into a staging file, then once more by `self-replace` next to the executable.
- Size limits (32 MiB for the download and the entry, 128 MiB inflated) bound disk and memory; the `zlib-rs` backend of `flate2` is the faster pure-Rust inflater and keeps four small packages out of the tree. Details and measurements: `update.md`.
- Binary size with `gdcli update` (local `cargo build --release --locked`, rustc 1.100.0): 3,812,312 bytes at the feature commit `4c0540a`, whose `Cargo.toml` version is 0.1.1 (+151,216, +4.13 % against 3,661,096 before); 3,812,248 bytes for the `v0.2.0` tag built the same way, 64 bytes less (the only change between the two commits is the version bump in `Cargo.toml` and `Cargo.lock`). Investigate any later increase above +5 % (3,844,150 bytes).

## Shipped binaries

Measured on 2026-11-19 from the GitHub release assets (`gh release download v0.1.1 --repo pycabbage/gdcli -p gdcli_linux_x86_64.tar.gz`, likewise for `aarch64` and for `v0.2.0`; extract the archive, `stat -c %s`). Both releases are built by the release workflow with `cargo zigbuild` (stripped, glibc baseline `GLIBC_2.30` for both targets in `v0.2.0`).

| Asset | v0.1.1 | v0.2.0 (first release with `gdcli update`) | Delta |
|---|---|---|---|
| x86_64 binary | 3,613,280 bytes | 3,768,576 bytes | +155,296 bytes, +4.30 % |
| aarch64 binary | 3,120,640 bytes | 3,260,096 bytes | +139,456 bytes, +4.47 % |
| x86_64 archive | 1,780,210 bytes | 1,862,914 bytes | +82,704 bytes, +4.65 % |
| aarch64 archive | 1,700,546 bytes | 1,778,208 bytes | +77,662 bytes, +4.57 % |

Both binary deltas are below +5 %. The threshold above is defined against the local `cargo build` baseline (3,661,096); here it is applied to the release-to-release delta of two `cargo zigbuild` builds, not to that figure.

## Not done on purpose

- No on-disk cache of API responses: results must be current.
- No parallel past-window search: the first window is usually enough, and the windows are nested so a failed window costs one small request.
- No `opt-level = "s"`: the owner asked for speed first.
