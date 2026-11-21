# Performance and memory

Priorities: startup time, resident memory, bytes transferred, number of round trips.

## Build

`[profile.release]`: `opt-level = 3`, `lto = "fat"`, `codegen-units = 1`, `panic = "abort"`, `strip = true`. The binary links only libc and libgcc dynamically (no OpenSSL; certificates are bundled through `webpki-roots`), so it cross-compiles with `zig cc` for x86_64 and aarch64 `linux-gnu`.

## Runtime decisions

- tokio `current_thread` runtime with only the `rt`, `macros` and `time` features (`time` for the overall deadline); no worker threads.
- Independent requests are concurrent with `tokio::try_join!` on one shared hyper client: past and upcoming lookups in `calendar list` and `meet list` (both Calendar only); space lookup and conference list in `meet get`, then participants, recordings, transcripts and smart notes of each conference record (four requests per record). `meet notes` and `meet transcript` are sequential on purpose (the chain is event, record list, one artifact list per record, then one export per document; with `--tab` each document adds the tab list and the web export), because there are one to three of each and a concurrent combinator would need a new dependency.
- The production client is HTTP/2-only (`http2_only(true)` on the hyper-util builder, ALPN `h2` on the rustls connector; every Google endpoint speaks h2), so concurrent requests are multiplexed over one TCP+TLS connection instead of each opening its own. Tests build their own protocol-negotiating client with a plain-HTTP connector for wiremock; production code uses `google::https_builder()` with the https-only connector (plain HTTP can never be reached in production). The same builder serves the OAuth token client.
- Timeouts (constants in `src/google.rs`): 10 s TCP connect timeout on the connector, and a 120 s deadline around every command (`with_deadline`; `auth setup` and `auth login` wait for the user and are exempt, but inside `auth login` the migration and the account discovery are bounded by it), failing with `error: timed out after 120s`.
- Every JSON request sends the `fields` partial-response parameter (the Markdown export has none); responses contain only printed fields. Responses are handled as `serde_json::Value`, with no typed models and no generated crates.
- `calendar list` upcoming requests `maxResults=5`; past windows keep a 5 element ring buffer instead of collecting a window. Range mode (`--from`) stops paging as soon as `--limit` rows (default 50, at most 1000) are collected, so its requests and memory are bounded by the limit: `maxResults=min(limit, 250)` for `calendar list`, 250 for `meet list` (the Meet predicate drops most events). `--query` adds one `q` parameter to the same requests and costs nothing locally.
- `docs get` writes each body frame to stdout as it arrives; the document is never held in memory. Only the error path buffers (a small JSON error body). `meet notes` and `meet transcript` keep the response headers of every document open (to fail before printing) and stream the bodies one after the other; each unread response buffers at most the HTTP/2 stream window of 2 MiB (5 MiB per connection, hyper's defaults); streaming itself never buffers a document. Limit: with four or more documents of about 1.7 MB or more, or more than about 100 concurrently open streams, the window or the server's stream limit could stall the first document until the 120 s deadline; real meetings have one to three documents, and the code is not hardened against it.
- Rendering writes directly into the caller's `impl Write`. `gdcli skills` writes the embedded files (read-only data in the binary) straight to stdout in a loop: no allocation, no file access, no network, no credentials; it is dispatched before the paths are resolved.
- Dependencies are pulled with default features off (clap, tokio, hyper-rustls, hyper-util, time, yup-oauth2). Only `auth login` and `auth setup` open a listening socket, start a browser flow or spawn a browser launcher (`open::that_detached`, called after the URL line was printed and only for an `https://` URL; the crate waits at most 250 ms per launcher and reaps it in a thread, so the login never waits for a launcher).
- Directories are created only when something is written (`auth login`, `auth setup` and the one-time migration create the data and `accounts/` directories; no other command creates any).
- Selecting the account costs one read of the small `default` pointer and two `stat` calls per command (`accounts/<email>.json` and the legacy `token.json`); the only extra request ever is the one-time `GET drive/v3/about` of the migration (and one per `auth login`, to identify the account), on the same HTTP/2 connection as the token refresh.
- `calendar list` and `meet list` send the `eventTypes` filter (default, focusTime, outOfOffice, fromGmail), so working-location and birthday entries are neither transferred nor parsed.

## Measurements (x86_64 Linux, release profile, rustc 1.100)

| Measurement | Before the REST layer (generated Google crates, chrono) | After the REST layer | Now (JSON, tabs, metadata, counts) | With `gdcli update` |
|---|---|---|---|---|
| Binary size (stripped) | 4,334,864 bytes | 3,565,016 bytes | 3,661,096 bytes | 3,812,312 bytes (+151,216, +4.13 %; aarch64 `cargo zigbuild`: 3,264,784 bytes); commit and `v0.2.0` figure: see the size bullet under `gdcli update`; with `GITHUB_TOKEN` support 3,816,200 bytes (+155,104, +4.24 %; aarch64 `cargo zigbuild`: 3,268,928 bytes), measured at the feature commit, whose `Cargo.toml` version is still 0.2.0; 3,816,264 bytes for the `v0.3.0` tag built the same way (see the size bullet under `gdcli update`) |
| Unique packages in the normal dependency tree, including gdcli (`cargo tree -e normal --prefix none --no-dedupe \| sed 's/ (.*//' \| sort -u \| wc -l`) | 123 | 108 | 109 (`foldhash` through `preserve_order`) | 121 (`self-replace`, `tempfile`, `tar`, `flate2`, `semver`, `zlib-rs`, `fastrand`, `rustix`, `linux-raw-sys`, `bitflags`, `filetime`, `getrandom` 0.4.3) |
| Peak RSS of `gdcli --help` (`/usr/bin/time -f %M`, 3 runs) | 3576, 3576, 3632 KB | 3468, 3388, 3376 KB | 3536, 3700, 3572 KB | 3848, 3760, 3784 KB |
| Peak RSS of a command with a real HTTPS round trip to Google and an invalid token (so the answer is a small 401) | 6.0 to 6.3 MB | 5.6 to 5.8 MB | `calendar list` 5728, `meet list` 5888, `docs get --tab` 5600, `docs metadata` 5888 KB | not re-measured (no change to the Google path); `gdcli update` against the real release: 6116 KB in the final run (6108 to 6288 KB over four runs; staging in the install directory, no tmpfs); 6192 KB (`/usr/bin/time -f %M`) when the self-update script of `release.md` updated a scratch 0.0.1 build to the genuine `v0.2.0` on 2026-11-19; 6288 KB in the end-to-end run against the genuine `v0.3.0` on 2026-11-20; 6648 KB (first run) and 6400 KB (second run) of the end-to-end block against the genuine `v0.4.0` on 2026-11-20 |
| TCP connections to port 443 per command (`strace -f -e trace=connect -o out gdcli <command>`, then `grep -c 'sin_port=htons(443)\|sin6_port=htons(443)' out`; invalid-token setup) | `calendar list` 2, `meet get` 4 (before HTTP/2-only) | 1 for every command | 1 for every command except `docs metadata`: 2, because Drive (`www.googleapis.com`) and the Docs API (`docs.googleapis.com`) are two hosts, one HTTP/2 connection each, run concurrently | not measured for `update` (it talks to `api.github.com`, `github.com` and `release-assets.githubusercontent.com`, three hosts, HTTP/2 only) |
| Source lines, `src/` (non-blank, after `cargo fmt`) | 1282 | 929 | 1324 (log in `code-reduction.md`) | 1591 (`src/update/` 263 non-blank in two files, `mod.rs` 129 and `fetch.rs` 134, 280 with blank lines; log in `code-reduction.md`) |

With an invalid token `meet get` and `docs get --tab` stop after the first request, so their connection counts for the full flows (events, space, records, artifacts; tab list, export and the redirect to the content host) have to be measured with a valid token. The tab export touches three hosts by design: `docs.googleapis.com` (tab list), `docs.google.com` (export) and the `googleusercontent.com` content host (the redirect target, fetched without the token).

Measuring a successful call needs real credentials and was not done here; the streaming and ring-buffer designs bound the memory of the success paths independent of document or calendar size. The `--past` / `--upcoming` counts bound the memory of the lists by N.

## `gdcli update`

- One HTTP/2 client from `google::https_builder()`; the archive is streamed from hyper into the hash and an unnamed file in the install directory (no whole-archive buffer, no tmpfs staging); only the API JSON (about 10 KB) is buffered. The extracted binary is copied once with `io::copy` into a staging file, then once more by `self-replace` next to the executable.
- Size limits (32 MiB for the download and the entry, 128 MiB inflated) bound disk and memory; the `zlib-rs` backend of `flate2` is the faster pure-Rust inflater and keeps four small packages out of the tree. Details and measurements: `update.md`.
- Binary size with `gdcli update` (local `cargo build --release --locked`, rustc 1.100.0): 3,812,312 bytes at the feature commit `4c0540a`, whose `Cargo.toml` version is 0.1.1 (+151,216, +4.13 % against 3,661,096 before); 3,812,248 bytes for the `v0.2.0` tag built the same way, 64 bytes less (the only change between the two commits is the version bump in `Cargo.toml` and `Cargo.lock`). With `GITHUB_TOKEN` support (the `bearer` and `github_token` helpers, the origin check and the 401 and rate-limit messages): 3,816,200 bytes at the feature commit, whose `Cargo.toml` version is still 0.2.0, +3,952 bytes against the `v0.2.0` figure and +4.24 % against 3,661,096; 3,816,264 bytes for the `v0.3.0` tag built the same way, 64 bytes more (the only change between the two commits is the version bump in `Cargo.toml` and `Cargo.lock`); no extra dependency; peak RSS of a real update: 6288 KB in the end-to-end run against the genuine `v0.3.0` and 6648 KB (first run) and 6400 KB (second run) against the genuine `v0.4.0`, about 6.0 to 6.6 MB (5,980 to 6,648 KB over all measured runs). Investigate any later increase above +5 % (3,844,150 bytes).

## Shipped binaries

Measured on 2026-11-19 and 2026-11-20 from the GitHub release assets (`gh release download v0.1.1 --repo pycabbage/gdcli -p gdcli_linux_x86_64.tar.gz`, likewise for `aarch64`, `v0.2.0`, `v0.3.0` and `v0.4.0`; extract the archive, `stat -c %s`). All releases are built by the release workflow with `cargo zigbuild` (stripped, glibc baseline `GLIBC_2.30` for both targets in `v0.2.0`, `v0.3.0` and `v0.4.0`).

| Asset | v0.1.1 | v0.2.0 (first release with `gdcli update`) | Delta |
|---|---|---|---|
| x86_64 binary | 3,613,280 bytes | 3,768,576 bytes | +155,296 bytes, +4.30 % |
| aarch64 binary | 3,120,640 bytes | 3,260,096 bytes | +139,456 bytes, +4.47 % |
| x86_64 archive | 1,780,210 bytes | 1,862,914 bytes | +82,704 bytes, +4.65 % |
| aarch64 archive | 1,700,546 bytes | 1,778,208 bytes | +77,662 bytes, +4.57 % |

| Asset | v0.2.0 | v0.3.0 (`GITHUB_TOKEN` support) | Delta |
|---|---|---|---|
| x86_64 binary | 3,768,576 bytes | 3,772,528 bytes | +3,952 bytes, +0.10 % |
| aarch64 binary | 3,260,096 bytes | 3,268,400 bytes | +8,304 bytes, +0.25 % |
| x86_64 archive | 1,862,914 bytes | 1,865,410 bytes | +2,496 bytes, +0.13 % |
| aarch64 archive | 1,778,208 bytes | 1,781,669 bytes | +3,461 bytes, +0.19 % |

All binary deltas are below +5 %. The threshold above is defined against the local `cargo build` baseline (3,661,096); here it is applied to the release-to-release delta of two `cargo zigbuild` builds, not to that figure. The local figures 3,816,200 bytes (feature commit `ac3a51c`, `Cargo.toml` version 0.2.0) and 3,816,264 bytes (`v0.3.0` tag, version 0.3.0) are `cargo build --release` sizes; the shipped sizes in this section are `cargo zigbuild` sizes of the release workflow. Both builds are stripped by the release profile and are built differently (`cargo build` versus `cargo zigbuild`), so the two kinds of figures are never compared with each other.

| Asset | v0.3.0 | v0.4.0 (accounts, scenario features, skills) | Delta |
|---|---|---|---|
| x86_64 binary | 3,772,528 bytes | 3,902,736 bytes | +130,208 bytes, +3.45 % |
| aarch64 binary | 3,268,400 bytes | 3,378,208 bytes | +109,808 bytes, +3.36 % |
| x86_64 archive | 1,865,410 bytes | 1,920,454 bytes | +55,044 bytes, +2.95 % |
| aarch64 archive | 1,781,669 bytes | 1,834,485 bytes | +52,816 bytes, +2.96 % |

Gate result for `v0.4.0`: the +5 % gate of a shipped binary is the previous shipped binary of the same architecture times 1.05. For x86_64 it is 3,961,154 bytes (3,772,528 times 1.05); the shipped `v0.4.0` binary is 3,902,736 bytes, margin 58,418 bytes. For aarch64 it is 3,431,820 bytes; the shipped binary is 3,378,208 bytes, margin 53,612 bytes. Both pass. The 3,844,150 bytes threshold of the `gdcli update` section belongs to the update work against the local baseline of 3,661,096 bytes and is not applied to later releases. The archives hold only `gdcli`; the skill text is part of the binary.

## Version 0.4.0 work packages (local `cargo build --release --locked`, rustc 1.100.0)

| State | x86_64 binary | Delta |
|---|---|---|
| `v0.3.0` tag | 3,816,264 bytes | baseline |
| accounts and the `auth` group (work package 1) | 3,872,504 bytes | +56,240 bytes, +1.47 % against `v0.3.0` |
| scenario features (work package 2: attachments, smart notes, range mode and `--query`, `meet notes`, `meet transcript`) | 3,913,408 bytes | +40,904 bytes against work package 1, +97,144 bytes, +2.55 % against `v0.3.0` |
| agent skill (work package 3: `gdcli skills`, 19,630 bytes of embedded text) | 3,950,568 bytes with the `0.4.0` manifest (3,950,504 bytes with the `0.3.0` manifest of the work package) | +37,096 bytes against work package 2 (both measured with the `0.3.0` manifest), +134,304 bytes, +3.52 % against `v0.3.0` (the +5 % gate is 4,007,077 bytes, margin 56,509 bytes) |

`cargo zigbuild --target aarch64-unknown-linux-gnu --release --locked` of work package 2: 3,353,528 bytes (the shipped `v0.3.0` aarch64 binary is 3,268,400 bytes: +85,128 bytes, +2.60 %; both are zigbuild sizes). Work package 3, shipped-style builds in a scratch copy of the final tree with the `0.4.0` manifest: x86_64 `cargo zigbuild --target x86_64-unknown-linux-gnu --release --locked`: 3,907,360 bytes (+134,832 bytes, +3.57 % against the shipped `v0.3.0` x86_64 binary of 3,772,528 bytes; gate 3,961,154 bytes, margin 53,794 bytes); aarch64 `cargo zigbuild --target aarch64-unknown-linux-gnu --release --locked`: 3,382,832 bytes (+114,432 bytes, +3.50 % against the shipped `v0.3.0` aarch64 binary of 3,268,400 bytes). The measurements of the work package were first taken with the `0.3.0` manifest (3,950,504 and 3,907,296 bytes for the x86_64 builds); the version bump of the release commit costs 64 bytes in both x86_64 builds and none in the aarch64 build. The shipped `v0.4.0` binaries (3,902,736 and 3,378,208 bytes) are 4,624 bytes smaller than these local zigbuild builds for both targets. The same local build with empty skill files is 3,925,784 bytes: the code of the command (value enum, help strings, dispatch, `src/skills.rs`, the writer parameter of `run_with`) costs 12,376 bytes and the 19,630 bytes of text 24,720 bytes (section alignment included; the 369 bytes added by the last review fix did not change any of the three binary sizes). The tree after the release, with the skill clarification of the next release (898 bytes more text, 20,528 bytes in all, `skills.md`), builds to the same sizes: 3,950,568 bytes (`cargo build --release --locked`), 3,907,360 bytes (x86_64 `cargo zigbuild`) and 3,382,832 bytes (aarch64 `cargo zigbuild`). No dependency was added: the normal tree still has 121 unique packages. Requests: `meet get` sends one more Meet request per conference record (smart notes, four per record instead of three); `calendar list` and `meet list` send the same requests as before plus an optional `q`, and range mode sends one request per 250 events read until the limit is reached.

## Version 0.4.1 (opening the login URL, local builds, rustc 1.100.0)

| Build | 0.4.0 tree (the `0.4.0` manifest) | 0.4.1 tree (the manifest still says `0.4.0`) | Delta | Against the shipped `v0.4.0` binary | +5 % gate (shipped times 1.05) |
|---|---|---|---|---|---|
| native `cargo build --release --locked` | 3,950,568 bytes | 4,035,344 bytes | +84,776 bytes, +2.15 % | not comparable (the shipped binaries are zigbuild builds) | not applied |
| x86_64 `cargo zigbuild --release --locked --target x86_64-unknown-linux-gnu` | 3,907,360 bytes | 3,996,232 bytes | +88,872 bytes, +2.27 % | +93,496 bytes, +2.40 % against 3,902,736 | 4,097,873 bytes, 101,641 bytes spare |
| aarch64 `cargo zigbuild --release --locked --target aarch64-unknown-linux-gnu` | 3,382,832 bytes | 3,461,712 bytes | +78,880 bytes, +2.33 % | +83,504 bytes, +2.47 % against 3,378,208 | 3,547,118 bytes, 85,406 bytes spare |

Both gates pass. The size comes from the `open` crate (+3 packages, 124 in all) and the code of the feature, not from text: a build of the same tree with the `v0.4.0` versions of `docs/setup.md` and `skills/gdcli` (12,454 bytes of text against 13,782 bytes now, +1,328: `docs/setup.md` +232, `SKILL.md` +333, `references/auth.md` +763) has exactly the same sizes in all three builds (4,035,344, 3,996,232 and 3,461,712 bytes, with different contents), so the embedded text costs nothing measurable in the file size. The figures `+88,088` and `+78,352` bytes (x86_64 and aarch64) come from the first prototype of the plan (3,995,448 and 3,461,184 bytes, local base 3,907,360 and 3,382,832); `+88,168` and `+78,368` come from its final prototype (3,995,528 and 3,461,200 bytes; native 4,034,640 bytes) and `+88,216` and `+78,416` from the reviewer's own rebuild of it. The real tree is +704 and +512 bytes above the final prototype; that difference is code (the prototype had shorter help strings, no `after_help`, another hint text), not skill text, and was not bisected further. The release commit's version bump costs about 64 bytes in the x86_64 builds. The maximum glibc symbol version of both zigbuild binaries stays `GLIBC_2.30`. Peak RSS of `gdcli --help` (`/usr/bin/time -f %M` inside `env -i`, 5 runs): 3856, 3720, 4088, 3844 and 3720 KB, against 3816, 3776, 3712, 3820 and 3856 KB for the 0.4.0 tree built the same way: no measurable difference (about +/- 0.2 MB between runs). No Google command, flag or request changed; the extra work exists only in `auth login` and `auth setup`: one environment lookup per listed name, one `https://` prefix check and, when the decision is `Open`, the launcher processes of the `open` crate.

## Not done on purpose

- No on-disk cache of API responses: results must be current.
- No parallel past-window search: the first window is usually enough, and the windows are nested so a failed window costs one small request.
- No `opt-level = "s"`: the owner asked for speed first.
