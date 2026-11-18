# Lints and formatting

- Formatting: `cargo fmt` with its defaults. There is no `rustfmt.toml` because no setting would carry information; `cargo fmt --check` gates CI.
- Lints live in `Cargo.toml` (`[lints]`), so the same configuration applies locally and in CI (`cargo clippy --all-targets --locked -- -D warnings`):
  - `rust::unsafe_code = "forbid"`; the crate contains no `unsafe`.
  - `clippy::pedantic` at `warn` with priority -1, so the explicit allows below win.
- Allowed pedantic lints and why:

| Lint | Reason |
|---|---|
| `missing_errors_doc`, `missing_panics_doc` | they demand `# Errors` / `# Panics` doc sections, but doc comments are forbidden in this repository (the documentation lives under `docs/`) |
| `module_name_repetitions` | pure naming noise (`events::list` style names are intended) |
| `must_use_candidate` | would add an attribute line to almost every public function of an internal library whose results are always used by the binary and the tests |

Every other pedantic finding was fixed in code without growing it: `match` on a bool became `if`/`else`, an appended `format!` became a rebuilt string, `to_owned` assignment became `clone_into`, the large `execute` future is boxed once (`Box::pin`), `meet get` was split into `details` and the printing code (it exceeded 100 lines), and test assertions use `assert_eq!(value, b"")` instead of `assert!(value.is_empty())`.
