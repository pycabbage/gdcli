# CI and release

## Files

- `.github/workflows/ci.yml`: build and test on commit push.
- `.github/workflows/release.yml`: GoReleaser release on tag push.
- `.github/workflows/actions.lock`: machine-generated lockfile pinning every action to a commit SHA (`gh actions-lock`).
- `.goreleaser.yaml`: GoReleaser configuration.
- `/dist` (GoReleaser output) must stay in `.gitignore`.

## CI workflow

- Trigger: `push` to any branch (`branches: ["**"]`). Tag pushes do not trigger it. No `pull_request` trigger: branches are tested on push.
- `concurrency` per ref with `cancel-in-progress`, so superseded runs are cancelled.
- `permissions: contents: read`.
- Steps, in order: `actions/checkout`, `actions/cache` (cargo registry, git db and `target`, keyed on `Cargo.lock`), `dtolnay/rust-toolchain@stable` with `components: rustfmt, clippy` (the action's documented input: "Comma-separated string of additional components to install"), `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo build --locked`, `cargo test --locked`. One job; the cheapest checks run first so a formatting or lint failure ends the run before the compile-heavy steps.
- `cargo fmt --check` and `cargo clippy ... -D warnings` are the format and lint gates: any formatting difference or any clippy warning fails CI. Lint levels and formatting options are configured in the repository (`[lints]` in `Cargo.toml`; rustfmt runs with its defaults, see `lints.md`), not in the workflow, so the same two commands give the same result locally; run them before pushing.
- `cargo build --locked` is kept although clippy type-checks and `cargo test` compiles: the owner asked for "build and test", and it is a separate, explicit check that the binary itself builds. It reuses the cached `target`, so it costs little.
- Tests use a local mock HTTP server and need no secrets.
- `CARGO_INCREMENTAL=0` keeps the cached `target` small.

## Release workflow

- Trigger: `push` of tags matching `v*`. Tags are created by the human owner (agents never create tags).
- Permissions (workflow level, one job): `contents: write` (create the release and upload assets with the built-in `GITHUB_TOKEN`), `id-token: write` and `attestations: write` (build provenance attestation, see Attestations). No secrets, no `artifact-metadata: write` (storage records exist only with `push-to-registry`, which is not used).
- Steps, in order:
  1. `actions/checkout` with `fetch-depth: 0` (GoReleaser needs full history and tags).
  2. `dtolnay/rust-toolchain@stable`.
  3. `cargo test --locked`: the gate; tag pushes do not run `ci.yml`. Format and clippy checks are deliberately not repeated here: they gate every branch push in `ci.yml`, and a release tag points to a commit that already passed CI.
  4. `mlugg/setup-zig@v2`: installs Zig, as in GoReleaser's own Rust/Zig workflow example (https://goreleaser.com/blog/rust-zig/: "we'll need this for both 'zig build' and 'cargo zigbuild'").
  5. `astral-sh/setup-uv@v10.3.3`: installs uv and puts `~/.local/bin` (the `uv tool` bin directory) on `PATH`.
  6. `uv tool install cargo-zigbuild`.
  7. `goreleaser/goreleaser-action@v7` with `version: "~> v2"` and `args: release --clean`, `GITHUB_TOKEN` from `secrets.GITHUB_TOKEN`.
  8. `actions/attest@v4` with `subject-checksums: ./dist/gdcli_checksums.txt`.
- Rust targets are not given to the toolchain action: GoReleaser's Rust docs state it "will run `rustup target add` for each target you declare".
- Why uv plus setup-zig: `cargo-zigbuild` finds Zig as `python3 -m ziglang` (or `$CARGO_ZIGBUILD_PYTHON_PATH`), else as `zig` (or `$CARGO_ZIGBUILD_ZIG_PATH`). `uv tool install cargo-zigbuild` puts the PyPI `ziglang` dependency in the tool's private venv, which the system `python3` cannot see, so alone it fails (`Error: Failed to find zig`, reproduced locally). `setup-zig` provides `zig` on `PATH`, which `cargo-zigbuild` then uses. The redundant `ziglang` wheel is harmless.
- `setup-zig@v2` is a mirror of https://codeberg.org/mlugg/setup-zig (upstream moved; the mirror is kept in sync and used by GoReleaser's docs). With no `version` input it installs the latest tagged Zig when there is no `build.zig.zon`.
- GoReleaser version policy: `version: "~> v2"` is exactly what both the goreleaser-action README and the GoReleaser GitHub Actions docs show. The action resolves it to the newest v2.x, so GoReleaser updates arrive without edits. The config starts with `version: 2`, GoReleaser's own compatibility marker for the whole v2 line. No pin is used because no document recommends one. The config uses `release.preflight` (needs 2.18 or later) and `use_existing_draft` (2.5 or later); `goreleaser check` was run with v2.20.2, the newest v2 at the time of writing.
- No tool versions are pinned (`cargo-zigbuild`, uv, Zig come at their latest). If a new tool release ever breaks the release job, pin that tool at that point (`uv tool install cargo-zigbuild==X`).

## Builder choice

- `builder: rust` (native GoReleaser Rust support, available since v2.5) runs `cargo zigbuild --target=<triple> --release --locked`.
- zigbuild cross-compiles `aarch64-unknown-linux-gnu` from the x86_64 runner. The dependency tree is pure Rust plus `ring` (C compiled by `zig cc`), so no OpenSSL or system cross toolchain is needed.
- Targets: `x86_64-unknown-linux-gnu` and `aarch64-unknown-linux-gnu` only. glibc baseline is zig's default; append e.g. `.2.31` to a target to pin it.
- `--locked` makes the release fail if `Cargo.lock` is out of date, so `Cargo.lock` must be committed.

## Release assets (contract used by README)

- `gdcli_linux_x86_64.tar.gz` and `gdcli_linux_aarch64.tar.gz`: no version in the name, so `https://github.com/pycabbage/gdcli/releases/latest/download/gdcli_linux_$(uname -m).tar.gz` always works.
- Each archive is tar.gz with the `gdcli` binary at the archive root (no wrapping directory) and nothing else (`files: [none*]` suppresses the default README/LICENSE inclusion; GoReleaser logs "no files matched glob=none*", which is expected). Entries are owned by root:root.
- `gdcli_checksums.txt`: SHA-256 checksums of both archives (version-free name too).
- The name template maps GoReleaser's `amd64`/`arm64` to `x86_64`/`aarch64` to match `uname -m`.
- GoReleaser also creates the GitHub Release with an auto-generated changelog.
- `releases/latest/download/<asset>` resolves once the release is published and not a draft or pre-release. GoReleaser leaves `make_latest` and `prerelease` at their defaults, so every published `v*` release becomes the latest one, including tags such as `v1.0.0-rc1`.

## Attestations

- Source: https://goreleaser.com/customization/publish/attestations/ ("If you're using GitHub Actions and want to attest your build artifacts, you can add the following to your release workflow") and the `actions/attest` README (v4.2.2, the current action; `actions/attest-build-provenance` is only its wrapper and the GoReleaser page uses `actions/attest@v4`).
- Implemented as documented: workflow permissions `id-token: write` and `attestations: write` (plus the `contents: write` GoReleaser needs), and `actions/attest@v4` after GoReleaser with `subject-checksums`. The docs example path `./dist/checksums.txt` is adapted to our checksum file name `./dist/gdcli_checksums.txt` (`checksum.name_template` in `.goreleaser.yaml`). The checksum file is in the `sha256sum` format the action requires (digest, two spaces, file name).
- What is attested: a SLSA build provenance attestation for each file listed in `gdcli_checksums.txt`, i.e. `gdcli_linux_x86_64.tar.gz` and `gdcli_linux_aarch64.tar.gz` (the exact bytes GoReleaser uploaded), signed through Sigstore with the workflow's OIDC identity and recorded in the repository's attestation store.
- Immutable releases: the attestation lives in GitHub's attestation store, not on the release. The step makes no Releases API call, so it never modifies the release or its assets, and it works whether or not immutability is enabled. It runs only after GoReleaser succeeded, that is after the release was published, so it always attests the final published bytes. This independent attestation is separate from the release attestation GitHub generates itself for an immutable release.
- `use_existing_draft` and `replace_existing_artifacts` only act inside GoReleaser, before publishing; a re-run after a failed upload replaces assets and only then reaches the attest step, which attests the new bytes.
- Limitation: if the attest step fails after a successful publish, the release exists (and may be immutable) without a provenance attestation, and re-running the workflow stops at the preflight check for an immutable release. Attestations can be created for the already published files by running the same attest step again in a separate job that downloads the release assets; the project has no such workflow.
- Verify a download (needs `gh`; `--owner pycabbage` is the form the GoReleaser docs use): `gh attestation verify gdcli_linux_x86_64.tar.gz --repo pycabbage/gdcli`.
- With immutable releases enabled, GitHub's own release attestation is checked with `gh release verify <tag>` and `gh release verify-asset <tag> <file>`.
- Not added to the README (the README is restricted to the install lines).

## Immutable releases

Works with and without the repository setting "Enable release immutability" (Settings, Releases; applies to future releases only). A published immutable release can no longer get assets added, replaced or deleted, and its tag can no longer be moved or deleted (and the tag name cannot be reused). GitHub recommends: create a draft, attach all assets, then publish.

- Mechanism: this is GoReleaser's own flow, so immutability needs no extra step and no extra permission beyond `contents: write` (GitHub creates its release attestation by itself at publish). In v2.20.2 (`internal/client/github.go`) `CreateRelease` always creates the release as a draft ("Always start with a draft release while uploading artifacts. PublishRelease will undraft it."). `internal/pipe/release/release.go` then uploads every uploadable artifact (both archives and the checksums file are in `ReleaseUploadableTypes`; extra files and `include_meta` metadata would be uploaded here too) and only after all uploads succeeded calls `PublishRelease`, which sets `draft: false`. `release.draft: true` is deliberately not set: it would stop GoReleaser before the publish call and require a hand-written publish step.
- Failure behaviour: any failed upload aborts before the publish call, so the release stays a draft (invisible to `releases/latest`, not immutable, no tag locked). Nothing is half-published.
- `use_existing_draft: true`: GitHub's get-release-by-tag endpoint returns published releases only, so without this option a re-run would create a second draft for the same tag. With it, the re-run finds the draft named after the tag and reuses it.
- `replace_existing_artifacts: true`: when an upload hits `422 already_exists` (assets left by the failed run), GoReleaser deletes that asset and uploads it again. Rebuilds are not byte-identical, so the retried assets come from the new build, and all of them are checked before publishing as one set.
- `preflight.fail_on_error: true` (GoReleaser 2.18+): the check that the tag is not already an immutable release runs before the build and aborts at once with `release vX already exists and is immutable, it cannot be updated`, instead of warning and failing after the build.
- Re-run after failure: fix the cause, then re-run the failed workflow run for the same tag (Actions, "Re-run all jobs", or `gh run rerun <run-id>`). The tag does not need to be recreated. If the release was already published (immutable), the tag cannot be reused: bump the version and push a new tag.
- Tag: created and pushed by the owner before the workflow starts; GoReleaser never creates it.
- Check the setting: `gh api repos/pycabbage/gdcli/immutable-releases` (200 enabled, 404 disabled; needs admin read access). After a release, `gh release verify <tag>` confirms it is immutable and `gh release verify-asset <tag> <file>` checks a downloaded asset.
- Tested locally against a mock GitHub API server (scratch, not in the repo) with GoReleaser v2.20.2 and this config: the call order was `POST release (draft)`, uploads of both archives and the checksums file, `PATCH draft=false`; an injected upload failure left the draft unpublished; the re-run reused the draft, replaced the existing assets and published; a further re-run on the published immutable release stopped in preflight. Not tested against real GitHub immutable releases (no admin access to the repository from the agent environment, and no releases may be created by agents).

## Action locking

- Workflows reference the latest major of each action: `actions/checkout@v7`, `actions/cache@v6`, `goreleaser/goreleaser-action@v7`, `actions/attest@v4`, `mlugg/setup-zig@v2`, `astral-sh/setup-uv@v10.3.3`, `dtolnay/rust-toolchain@stable`. Verify the latest major with `gh api repos/<owner>/<repo>/releases/latest --jq .tag_name` and `gh api --paginate "repos/<owner>/<repo>/tags?per_page=100" --jq '.[].name' | grep -E '^v[0-9]+$'`.
- Two actions have no floating major tag, so they cannot be written as `@vN`:
  - `astral-sh/setup-uv`: since v8 only immutable full tags exist (`v10.3.3`; there is no `v10`, `v9` or `v8` tag, the last floating tag is `v7`). Its README shows the SHA of the full tag. The ref `@v10.3.3` states the latest major (10); update it to the newest full tag by editing the ref and re-locking.
  - `dtolnay/rust-toolchain`: the revision selects the toolchain (README: "The selection of Rust toolchain is made based on the particular @rev of this Action"). `@stable` is a branch that the author advances with each Rust release, and the only major tag, `v1`, selects nothing. The lockfile records the branch as `ref: 'stable'` with the commit SHA of the branch head at lock time.
- A moved branch ref makes the lockfile stale, not wrong: it keeps pointing at the locked commit until `--relock` advances it.
- `gh actions-lock` (extension `github/gh-actions-lock`, install with `gh extension install github/gh-actions-lock`) resolves each ref to an immutable commit SHA and records it in `.github/workflows/actions.lock`. Workflows keep the readable `@vN` refs; the lockfile is what pins them.
- `.github/workflows/actions.lock` and `Cargo.lock` are tool output and are never hand-edited. Their machine-generated `#` headers (`This file is machine-generated by gh actions-lock...`, `This file is automatically @generated by Cargo.`) are part of that output and are the only comment-like text allowed in the repo.
- `--no-narrow` is always used. The plain default run (`gh actions-lock --no-interactive`, tested in a scratch repo) rewrites the workflows with full semver refs: `actions/checkout@v7` becomes `@v7.0.1`, `actions/cache@v6` becomes `@v6.1.0`, `goreleaser/goreleaser-action@v7` becomes `@v7.3.0`, and the lockfile keys become those refs. The project rule is that workflows state the latest major (`@v7`), so the official `--no-narrow` option keeps the major refs in the workflows and pins them in the lockfile only.
- Because the pin is for a moving major tag, `--relock` is how the lockfile follows new minor/patch releases of the same major.

Lock, first time or after adding/changing an action:

```
gh actions-lock --no-narrow --no-interactive
```

Update the pinned commits to where the moving refs (`v7`, `v6`, `v4`, `v2`, `stable`) point now:

```
gh actions-lock --no-narrow --no-interactive --relock
```

Moving to a new major (e.g. `checkout@v8`): edit the `uses:` ref in the workflows, then run the first lock command above; the tool pins the new ref and drops the unused entry.

Verify (read-only; offline variant has no network):

```
gh actions-lock --verify
gh actions-lock --verify-local
```

## Repository requirements

- GitHub Actions enabled; workflow permissions may stay at the read-only default, since `release.yml` requests `contents: write`, `id-token: write` and `attestations: write` itself.
- No secrets or variables to configure.
- Not verified on GitHub (only locally): the workflows themselves, including `setup-zig`, `setup-uv`, `uv tool install cargo-zigbuild` on a runner and the attest step. The first tag run is the first real test.
- The lockfile and workflows must be committed together.
- Release procedure: push the commit, wait for CI green, the owner creates and pushes a `vX.Y.Z` tag, the release workflow creates a draft release, attaches the assets and publishes it.
