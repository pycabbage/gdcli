# gdcli

Agent-oriented CLI for Google Workspace: Meet, Calendar and Docs. Rust. This file exists because the owner's INSTRUCTIONS.md is not committed; it transcribes every rule from it.

## Commands

```
gdcli setup              auth setup wizard: have the user place the key, then start the OAuth login flow
gdcli login              OAuth login flow only (setup without the wizard part)
gdcli meet list          latest 10 Google Meet meetings (default past 5, future 5; --past N --upcoming N, alias --future)
gdcli meet get <id>      show Meet meeting info
gdcli calendar list      latest 10 calendar events (default past 5, future 5; --past N --upcoming N, alias --future)
gdcli calendar get <id>  get and show an event
gdcli docs get <id>      get a Google Doc as Markdown; --tab <id|name> selects one tab
gdcli docs metadata <id> metadata of a Google Doc
```

- `--json` (same information as JSON) exists on meet/calendar list and get and on `docs metadata`, never on `docs get`.
- `docs get`: use an official Markdown-output API if one exists, otherwise write own Markdown conversion. Option `--json` (raw JSON output) exists only if no official Markdown API exists and raw JSON is available; otherwise drop the option.
- Current decision: Drive `files.export` with `text/markdown` is the official Markdown API, so `docs get` has NO `--json`.

## Rules

- Agents may run `git add`, `commit`, `push` and `tag`; never `git config`, never force-push, never rewrite or delete a tag whose release was published.
- Pursue performance above all; optimize memory usage.
- User-touched config (e.g. client_secret.json) lives in `$XDG_CONFIG_HOME/gdcli/`; program-touched state/cache (e.g. token) lives in `$XDG_DATA_HOME/gdcli/`.
- No code comments at all, in any file type (including YAML/TOML). Needed information goes in English docs under `<repo_root>/docs/{design,...}`.
- README.md is for users only, in English: exactly one line describing the project plus one 3-line code block (download from releases, extract into `~/.local/bin`, run `gdcli setup`). No contribution info or any other text.
- Comment/README limits are never a reason to drop information from the repo. Keep the repo minimal in bytes yet complete in information (all information in docs/AGENTS.md).
- Split source into appropriately sized modules/hierarchy.
- Write as little source as possible. Search exhaustively (dozens of times) for reliable libraries/frameworks; hand-write code only if none exists.
- After implementing, an independent reviewer (separate from tests and from the implementer) must review it.
- CI workflow: on commit push, build and test.
- Release workflow: on tag push, GoReleaser release for linux amd64 and linux arm64.
- Every workflow must use the latest major version of each action (e.g. `actions/checkout@v7`) and be locked with `gh actions-lock`.
- Never hand-edit tool-generated files (`.github/workflows/actions.lock`, `Cargo.lock`) without a strong reason; the machine-generated headers of `actions.lock` and `Cargo.lock` are the only comment-like text allowed.
- Every GitHub Action used in `.github/workflows` (first-party or third-party) must have at least 1000 stars, checked with `gh api repos/<owner>/<repo> --jq .stargazers_count`; transitive actions inside an accepted wrapper are recorded in `docs/design/release.md`.
- Commit only after the reviewer passes.
- Git: author/credentials are preconfigured. NEVER run any `git config` command; NEVER read or write `.git/config`.
- Tags and releases are managed by agents (tag = `v` + Cargo.toml version; procedure in `docs/design/release.md`). End-to-end tests with real credentials run only with the owner's installed credentials.
- Violating these instructions may get the deliverables deleted.

## Docs index

- `docs/setup.md`
- `docs/design/`
