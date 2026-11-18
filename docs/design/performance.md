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

## Measurements (x86_64 Linux, release profile, rustc 1.99)

| Measurement | Before (generated Google crates, chrono) | Now (own REST layer, time) |
|---|---|---|
| Binary size (stripped) | 4,334,864 bytes | 3,565,016 bytes (-17.8 %) |
| Unique packages in the normal dependency tree, including gdcli (`cargo tree -e normal --prefix none --no-dedupe \| sed 's/ (.*//' \| sort -u \| wc -l`) | 123 | 108 |
| Peak RSS of `gdcli --help` (`/usr/bin/time -f %M`, 3 runs) | 3576, 3576, 3632 KB | 3468, 3388, 3376 KB |
| Peak RSS of a command with a real HTTPS round trip to Google and an invalid token (so the answer is a small 401) | 6.0 to 6.3 MB | `calendar list` 5764, `meet list` 5724, `docs get` 5616, `meet get` 5652 KB |
| TCP connections to port 443 per command (`strace -f -e trace=connect -o out gdcli <command>`, then `grep -c 'sin_port=htons(443)\|sin6_port=htons(443)' out`; invalid-token setup) | `calendar list` 1 (2 before HTTP/2-only), `meet get` 4 (before HTTP/2-only) | 1 for every command above |
| Source lines, `src/` (non-blank, after `cargo fmt`) | 1282 | 929 (log in `code-reduction.md`) |

With an invalid token `meet get` stops after the first request, so the connection count for the full `meet get` flow (events, space, records, artifacts) has to be measured with a valid token.

Measuring a successful call needs real credentials and was not done here; the streaming and ring-buffer designs bound the memory of the success paths independent of document or calendar size.

## Not done on purpose

- No on-disk cache of API responses: results must be current.
- No parallel past-window search: the first window is usually enough, and the windows are nested so a failed window costs one small request.
- No `opt-level = "s"`: the owner asked for speed first.
