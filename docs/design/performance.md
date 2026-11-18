# Performance and memory

Priorities: startup time, resident memory, bytes transferred, number of round trips.

## Build

`[profile.release]`: `opt-level = 3`, `lto = "fat"`, `codegen-units = 1`, `panic = "abort"`, `strip = true`. The binary links only libc and libgcc dynamically (no OpenSSL; certificates are bundled through `webpki-roots`), so it cross-compiles with `zig cc` for x86_64 and aarch64 `linux-gnu`.

## Runtime decisions

- tokio `current_thread` runtime with only the `rt` and `macros` features; no worker threads.
- Independent requests are concurrent with `tokio::try_join!` on one shared hyper client (HTTP/2 multiplexing when Google negotiates it): past and upcoming lookups in `calendar list` and `meet list`; space and conference list in `meet get <code>`; the record-to-space chain, participants, recordings and transcripts in `meet get <record>`.
- Every request sends the `fields` partial-response parameter; responses contain only printed fields.
- `calendar list` upcoming requests `maxResults=5`; past windows keep a 5 element ring buffer instead of collecting a window.
- `docs get` writes each body frame to stdout as it arrives; the document is never held in memory. Only the error path buffers (a small JSON error body).
- Rendering writes directly into the caller's `impl Write`.
- Dependencies are pulled with default features off (clap, google-calendar3, google-drive3, google-apis-common, tokio, hyper-rustls, hyper-util, yup-oauth2, chrono). Non-auth commands never open a listening socket and never start a browser flow.
- Directories are created only when something is written.

## Measurements (x86_64 Linux, release profile, rustc 1.99)

| Measurement | Value |
|---|---|
| Binary size (stripped) | 4,332,160 bytes (4.1 MiB) |
| Unique packages in the normal dependency tree, including gdcli (`cargo tree -e normal --prefix none --no-dedupe \| sed 's/ (.*//' \| sort -u \| wc -l`) | 123 |
| Peak RSS of `gdcli --help` | about 3.6 MB (`/usr/bin/time -f %M`, 3 runs: 3712, 3648, 3568 KB) |
| Peak RSS of a command with a real HTTPS round trip to Google (`calendar list`, `meet list`, `docs get` with an invalid token, so the response is a small 401 JSON) | about 6.1 to 6.4 MB, 0.06 to 0.19 s wall clock |

Measuring a successful call needs real credentials and was not done; the streaming and ring-buffer designs bound the memory of the success paths independent of document or calendar size.

## Not done on purpose

- No on-disk cache of API responses: results must be current.
- No parallel past-window search: the first window is usually enough, and the windows are nested so a failed window costs one small request.
- No `opt-level = "s"`: the owner asked for speed first.
