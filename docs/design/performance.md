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

| Measurement | Before the REST layer (generated Google crates, chrono) | After the REST layer | Now (JSON, tabs, metadata, counts) |
|---|---|---|---|
| Binary size (stripped) | 4,334,864 bytes | 3,565,016 bytes | 3,661,096 bytes |
| Unique packages in the normal dependency tree, including gdcli (`cargo tree -e normal --prefix none --no-dedupe \| sed 's/ (.*//' \| sort -u \| wc -l`) | 123 | 108 | 109 (`foldhash` through `preserve_order`) |
| Peak RSS of `gdcli --help` (`/usr/bin/time -f %M`, 3 runs) | 3576, 3576, 3632 KB | 3468, 3388, 3376 KB | 3536, 3700, 3572 KB |
| Peak RSS of a command with a real HTTPS round trip to Google and an invalid token (so the answer is a small 401) | 6.0 to 6.3 MB | 5.6 to 5.8 MB | `calendar list` 5728, `meet list` 5888, `docs get --tab` 5600, `docs metadata` 5888 KB |
| TCP connections to port 443 per command (`strace -f -e trace=connect -o out gdcli <command>`, then `grep -c 'sin_port=htons(443)\|sin6_port=htons(443)' out`; invalid-token setup) | `calendar list` 2, `meet get` 4 (before HTTP/2-only) | 1 for every command | 1 for every command except `docs metadata`: 2, because Drive (`www.googleapis.com`) and the Docs API (`docs.googleapis.com`) are two hosts, one HTTP/2 connection each, run concurrently |
| Source lines, `src/` (non-blank, after `cargo fmt`) | 1282 | 929 | 1324 (log in `code-reduction.md`) |

With an invalid token `meet get` and `docs get --tab` stop after the first request, so their connection counts for the full flows (events, space, records, artifacts; tab list, export and the redirect to the content host) have to be measured with a valid token. The tab export touches three hosts by design: `docs.googleapis.com` (tab list), `docs.google.com` (export) and the `googleusercontent.com` content host (the redirect target, fetched without the token).

Measuring a successful call needs real credentials and was not done here; the streaming and ring-buffer designs bound the memory of the success paths independent of document or calendar size. The `--past` / `--upcoming` counts bound the memory of the lists by N.

## Not done on purpose

- No on-disk cache of API responses: results must be current.
- No parallel past-window search: the first window is usually enough, and the windows are nested so a failed window costs one small request.
- No `opt-level = "s"`: the owner asked for speed first.
