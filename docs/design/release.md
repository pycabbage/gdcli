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

- Trigger: `push` of tags matching `v*`. Tags are created and pushed by agents (see Release management).
- Permissions (workflow level, one job): `contents: write` (create the release and upload assets with the built-in `GITHUB_TOKEN`), `id-token: write` and `attestations: write` (build provenance attestation, see Attestations). No secrets, no `artifact-metadata: write` (storage records exist only with `push-to-registry`, which is not used).
- Steps, in order:
  1. `actions/checkout` with `fetch-depth: 0` (GoReleaser needs full history and tags).
  2. `dtolnay/rust-toolchain@stable`.
  3. `cargo test --locked`: the gate; tag pushes do not run `ci.yml`. Format and clippy checks are deliberately not repeated here: they gate every branch push in `ci.yml`, and a release tag points to a commit that already passed CI.
  4. `pip install cargo-zigbuild`: the install documented in the cargo-zigbuild README ("pip install cargo-zigbuild ... will also install ziglang automatically"). No action, no uv, no separate Zig install, no `PATH` handling (see below).
  5. `goreleaser/goreleaser-action@v7` with `version: "~> v2"` and `args: release --clean`, `GITHUB_TOKEN` from `secrets.GITHUB_TOKEN`.
  6. `actions/attest-build-provenance@v4` with `subject-checksums: ./dist/gdcli_checksums.txt` (see Attestations).
- Rust targets are not given to the toolchain action: GoReleaser's Rust docs state it "will run `rustup target add` for each target you declare".
- Why plain `pip`: `cargo-zigbuild` finds Zig as `python3 -m ziglang` first (or `$CARGO_ZIGBUILD_PYTHON_PATH`), else as `zig` (or `$CARGO_ZIGBUILD_ZIG_PATH`) (`src/zig/locate.rs`). The GitHub `ubuntu-latest` image is built so that bare `pip` works (`images/ubuntu/scripts/build/install-python.sh` in actions/runner-images): it writes `/etc/pip.conf` with `break-system-packages = true` ("allow user to continue using pip") and puts `$HOME/.local/bin` on `PATH` ("make installed pip commands immediately available"). As a non-root user `pip install` falls back to the user site, which the system `python3` imports, so `python3 -m ziglang` finds the `ziglang` wheel that `cargo-zigbuild` depends on. Confirmed by the v0.1.1 release run on `ubuntu-26.04`: `Defaulting to user installation because normal site-packages is not writeable`, `Successfully installed cargo-zigbuild-0.24.1 ziglang-0.17.1`, and GoReleaser's `cargo zigbuild` then built both targets.
- Why not uv or a Zig action: uv is not on the runner image (the image readmes list Python 3.12.3 with pip 24.0 for Ubuntu 24.04 and Python 3.14.4 with pip 25.1.1 for Ubuntu 26.04, plus pipx, jq and Rust, and no uv and no Zig: `images/ubuntu/Ubuntu2404-Readme.md` image 20261111.370.1 and `Ubuntu2604-Readme.md` image 20261111.196.1 (the v0.1.0 run; the v0.1.1 run used image 20261111.82.6, whose readme could not be retrieved) in actions/runner-images, as of those image versions; the run logs print the image name and version, not the Python version), `astral-sh/setup-uv` has fewer than 1000 stars (Action policy), and `uv tool install cargo-zigbuild` alone cannot work: the `ziglang` wheel lands in uv's private tool venv, which the system `python3` cannot see, and the wheel exposes only `python-zig`, never a `zig` executable, so `cargo-zigbuild` fails with `Failed to find zig` (reproduced locally). Making that work needs `CARGO_ZIGBUILD_ZIG_PATH=python-zig` plus an extra uv install route, which is more than the single documented pip line. `mlugg/setup-zig` has 241 stars.
- GoReleaser version policy: `version: "~> v2"` is exactly what both the goreleaser-action README and the GoReleaser GitHub Actions docs show. The action resolves it to the newest v2.x, so GoReleaser updates arrive without edits. The config starts with `version: 2`, GoReleaser's own compatibility marker for the whole v2 line. No pin is used because no document recommends one. The config uses `release.preflight` (needs 2.18 or later) and `use_existing_draft` (2.5 or later); `goreleaser check` passes with v2.20.3. The releases up to `v0.3.0` ran GoReleaser v2.20.2; the `v0.4.0` run resolved `~> v2` to v2.20.3 (published 2026-11-20T04:31:19Z) without any edit of the workflow.
- No tool versions are pinned (`cargo-zigbuild` and the `ziglang` wheel come at their latest). If a new tool release ever breaks the release job, pin that tool at that point (`pip install cargo-zigbuild==X`).

## Builder choice

- `builder: rust` (native GoReleaser Rust support, available since v2.5) runs `cargo zigbuild --target=<triple> --release --locked`.
- zigbuild cross-compiles `aarch64-unknown-linux-gnu` from the x86_64 runner. The dependency tree is pure Rust plus `ring` (C compiled by `zig cc`), so no OpenSSL or system cross toolchain is needed.
- Targets: `x86_64-unknown-linux-gnu` and `aarch64-unknown-linux-gnu` only. glibc baseline is zig's default; append e.g. `.2.31` to a target to pin it.
- `--locked` makes the release fail if `Cargo.lock` is out of date, so `Cargo.lock` must be committed.

## Release assets (contract used by README and by `gdcli update`)

- `gdcli_linux_x86_64.tar.gz` and `gdcli_linux_aarch64.tar.gz`: no version in the name, so `https://github.com/pycabbage/gdcli/releases/latest/download/gdcli_linux_$(uname -m).tar.gz` always works.
- Each archive is tar.gz with the `gdcli` binary at the archive root (no wrapping directory) and nothing else (`files: [none*]` suppresses the default README/LICENSE inclusion; GoReleaser logs "no files matched glob=none*", which is expected). Entries are owned by root:root.
- `gdcli_checksums.txt`: SHA-256 checksums of both archives (version-free name too).
- The name template maps GoReleaser's `amd64`/`arm64` to `x86_64`/`aarch64` to match `uname -m`.
- GoReleaser also creates the GitHub Release with an auto-generated changelog.
- `releases/latest/download/<asset>` resolves once the release is published and not a draft or pre-release. `.goreleaser.yaml` sets `prerelease: auto` under `release:`, so a tag with a semver prerelease suffix such as `v1.0.0-rc1` is published as a GitHub prerelease and is neither `releases/latest` nor the target of `releases/latest/download`; every other published `v*` release becomes the latest one (`make_latest` stays at its default). Evidence (GoReleaser v2.20.2): `internal/pipe/release/release.go` lines 75-84 (`case "auto": if ctx.Semver.Prerelease != "" { ctx.PreRelease = true }`), `internal/client/github.go` line 544 (the `prerelease` and `make_latest` fields of the release request), and a run of GoReleaser against a mock GitHub API (scratch, not in the repository): tag `v1.0.0-rc1` sent `POST releases` with `draft: true, prerelease: true` and then `PATCH` with `draft: false, prerelease: true, make_latest: "false"`; tag `v1.0.0` sent `prerelease: false`. `goreleaser check` accepts the key. No prerelease tag has been published (an immutable tag cannot be reused), so the first real one is the live test; `v0.2.0` is the first release published with the key in place and is correctly a normal release (`isPrerelease: false`, and `releases/latest/download` resolves to it). `gdcli update` also refuses a prerelease tag by itself.
- `gdcli update` (see `update.md`) reads these assets from the API: the asset names above, the entry `gdcli` at the root of the archive, a `sha256:` digest on every asset and `immutable: true` on the release. A change of any of them (archive layout, names, immutability) breaks self-update and must be accompanied by a change of `src/update/`.
- GitHub picks `releases/latest` by the creation time of the release (`created_at`), not by semver. Publish versions in increasing order; a backport published after a newer release becomes "latest" and `gdcli update` then reports `up_to_date` for users who already run the newer version.

## Attestations

- Source: https://goreleaser.com/customization/publish/attestations/ ("If you're using GitHub Actions and want to attest your build artifacts, you can add the following to your release workflow") and the `actions/attest` README. The GoReleaser page uses `actions/attest@v4`; this repository uses `actions/attest-build-provenance@v4` instead, because of the 1000-star rule (see Action policy).
- Implemented as documented: workflow permissions `id-token: write` and `attestations: write` (plus the `contents: write` GoReleaser needs), and `actions/attest-build-provenance@v4` after GoReleaser with `subject-checksums` (the input is passed through unchanged). The docs example path `./dist/checksums.txt` is adapted to our checksum file name `./dist/gdcli_checksums.txt` (`checksum.name_template` in `.goreleaser.yaml`). The checksum file is in the `sha256sum` format the action requires (digest, two spaces, file name).
- Accepted exception: `actions/attest-build-provenance@v4` (1077 stars) is a composite wrapper (since v4 "simply a wrapper on top of `actions/attest`"; its README says new implementations should use `actions/attest` directly) that runs `actions/attest@508db95...` (v4.2.1; 187 stars). The workflow names only the wrapper, which meets the 1000-star rule; the transitive `actions/attest` is the same GitHub-owned code the GoReleaser docs use directly, and `gh actions-lock` records it as a transitive dependency of the wrapper. The other candidates fail the rule as well (`slsa-framework/slsa-github-generator` 611 stars, `sigstore/cosign-installer` 214 stars) and `gh attestation` can only verify, not create. This exception is accepted.
- What is attested: a SLSA build provenance attestation for each file listed in `gdcli_checksums.txt`, i.e. `gdcli_linux_x86_64.tar.gz` and `gdcli_linux_aarch64.tar.gz` (the exact bytes GoReleaser uploaded), signed through Sigstore with the workflow's OIDC identity and recorded in the repository's attestation store.
- Immutable releases: the attestation lives in GitHub's attestation store, not on the release. The step makes no Releases API call, so it never modifies the release or its assets, and it works whether or not immutability is enabled. It runs only after GoReleaser succeeded, that is after the release was published, so it always attests the final published bytes. This independent attestation is separate from the release attestation GitHub generates itself for an immutable release.
- `use_existing_draft` and `replace_existing_artifacts` only act inside GoReleaser, before publishing; a re-run after a failed upload replaces assets and only then reaches the attest step, which attests the new bytes.
- Limitation: if the attest step fails after a successful publish, the release exists (and may be immutable) without a provenance attestation, and re-running the workflow stops at the preflight check for an immutable release. Attestations can be created for the already published files by running the same attest step again in a separate job that downloads the release assets; the project has no such workflow.
- Verify a download (needs `gh`; `--owner pycabbage` is the form the GoReleaser docs use): `gh attestation verify gdcli_linux_x86_64.tar.gz --repo pycabbage/gdcli`.
- With immutable releases enabled, GitHub's own release attestation is checked with `gh release verify <tag>` and `gh release verify-asset <tag> <file>`.
- Not added to the README (the README holds the install lines and the skills note only).

## Immutable releases

Works with and without the repository setting "Enable release immutability" (Settings, Releases; applies to future releases only). A published immutable release can no longer get assets added, replaced or deleted, and its tag can no longer be moved or deleted (and the tag name cannot be reused). GitHub recommends: create a draft, attach all assets, then publish.

- Mechanism: this is GoReleaser's own flow, so immutability needs no extra step and no extra permission beyond `contents: write` (GitHub creates its release attestation by itself at publish). In v2.20.2 (`internal/client/github.go`) `CreateRelease` always creates the release as a draft ("Always start with a draft release while uploading artifacts. PublishRelease will undraft it."). `internal/pipe/release/release.go` then uploads every uploadable artifact (both archives and the checksums file are in `ReleaseUploadableTypes`; extra files and `include_meta` metadata would be uploaded here too) and only after all uploads succeeded calls `PublishRelease`, which sets `draft: false`. `release.draft: true` is deliberately not set: it would stop GoReleaser before the publish call and require a hand-written publish step.
- Failure behaviour: any failed upload aborts before the publish call, so the release stays a draft (invisible to `releases/latest`, not immutable, no tag locked). Nothing is half-published.
- `use_existing_draft: true`: GitHub's get-release-by-tag endpoint returns published releases only, so without this option a re-run would create a second draft for the same tag. With it, the re-run finds the draft named after the tag and reuses it.
- `replace_existing_artifacts: true`: when an upload hits `422 already_exists` (assets left by the failed run), GoReleaser deletes that asset and uploads it again. Rebuilds are not byte-identical, so the retried assets come from the new build, and all of them are checked before publishing as one set.
- `preflight.fail_on_error: true` (GoReleaser 2.18+): the check that the tag is not already an immutable release runs before the build and aborts at once with `release vX already exists and is immutable, it cannot be updated`, instead of warning and failing after the build.
- Recovery after a failed run, and the rule for a published release: see Release management. A published release (immutable or not) is never reused.
- Tag: created and pushed before the workflow starts (the push triggers it); GoReleaser never creates it.
- Check the setting: `gh api repos/pycabbage/gdcli/immutable-releases` (200 enabled, 404 disabled; needs admin read access). After a release, `gh release verify <tag>` confirms it is immutable and `gh release verify-asset <tag> <file>` checks a downloaded asset.
- Verified on GitHub: releases v0.1.0 and v0.1.1 were created as drafts, got all three assets, were published by GoReleaser and both report `isImmutable: true`; `gh release verify v0.1.0` and `gh release verify v0.1.1` pass (both run logs show the draft URL `releases/tag/untagged-...` followed by `release published ... releases/tag/<tag>`). The failure paths (an upload that fails before publish, the re-run with `use_existing_draft` and `replace_existing_artifacts`, the preflight stop on a published immutable release) were only tested locally against a mock GitHub API server (scratch, not in the repo) with GoReleaser v2.20.2 and this config: the call order was `POST release (draft)`, uploads of both archives and the checksums file, `PATCH draft=false`; an injected upload failure left the draft unpublished; the re-run reused the draft, replaced the existing assets and published; a further re-run on the published immutable release stopped in preflight.

## Action policy

- Rule (owner): Every GitHub Action used in `.github/workflows` (first-party or third-party) must have at least 1000 stars, checked with `gh api repos/<owner>/<repo> --jq .stargazers_count`; transitive actions inside an accepted wrapper are recorded in docs/design/release.md. Check before adding an action and whenever one is updated. Prefer a documented tool invocation in a `run:` step over a low-star action.
- Stars measured on 2026-11-18:

| Action | Stars | Last release | Status |
|---|---|---|---|
| `actions/checkout` | 9025 | v7.0.1, 2026-07-20 | used |
| `actions/cache` | 5604 | v6.1.0, 2026-06-26 | used |
| `dtolnay/rust-toolchain` | 1605 | `v1` 2022-07-15 (branch `stable` active, pushed 2026-11-15) | used |
| `goreleaser/goreleaser-action` | 1037 | v7.3.0, 2026-10-12 | used (37 stars above the limit) |
| `actions/attest-build-provenance` | 1077 | v4.2.2, 2026-08-06 | used, wrapper (see Attestations) |
| `actions/attest` | 187 | v4.2.2, 2026-08-04 | transitive only, runs inside the wrapper |
| `astral-sh/setup-uv` | 913 | v10.3.3, 2026-10-28 | removed |
| `mlugg/setup-zig` (GitHub mirror of a Codeberg repository) | 241 | v2.2.1, 2026-01-19 | removed |
| `slsa-framework/slsa-github-generator` | 611 | v2.1.0, 2025-02-24 | evaluated for attestations, not used |
| `sigstore/cosign-installer` | 214 | v4.1.3, 2026-11-04 | evaluated for attestations, not used |

- Zig is provided by `pip install cargo-zigbuild` (see Release workflow), so no uv and no Zig action are needed.
- Accepted exception for transitive actions: the wrapper `actions/attest-build-provenance` (1077 stars) is the action named in the workflow and meets the rule; it runs `actions/attest@508db95...` (v4.2.1, 187 stars) internally, which is recorded here and in the lockfile as its transitive dependency. See Attestations for the numbers and the alternatives that were evaluated.

## Action locking

- Workflows reference the latest major of each action: `actions/checkout@v7`, `actions/cache@v6`, `goreleaser/goreleaser-action@v7`, `actions/attest-build-provenance@v4`, `dtolnay/rust-toolchain@stable`. Verify the latest major with `gh api repos/<owner>/<repo>/releases/latest --jq .tag_name` and `gh api --paginate "repos/<owner>/<repo>/tags?per_page=100" --jq '.[].name' | grep -E '^v[0-9]+$'`.
- `dtolnay/rust-toolchain` cannot be written as `@vN`: the revision selects the toolchain (README: "The selection of Rust toolchain is made based on the particular @rev of this Action"). `@stable` is a branch that the author advances with each Rust release, and the only major tag, `v1`, selects nothing. The lockfile records the branch as `ref: 'stable'` with the commit SHA of the branch head at lock time.
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

## Release management

Who: agents manage tags and releases. The human owner pushes only exceptionally.

Tag policy:

- `vX.Y.Z`, annotated (`git tag -a`), always equal to `v` plus the `version` in `Cargo.toml`, so that `gdcli --version` (clap reads `CARGO_PKG_VERSION`) always matches the tag. The release workflow triggers on `v*`.
- SemVer. While the version is 0.x: patch for fixes and for a new option or environment variable on an existing command, with existing options, record keys and exit statuses unchanged; minor for new commands and breaking changes. `v0.2.0` (a new command) and `v0.3.0` (a small addition to `gdcli update`) were released under the earlier wording (patch for fixes, minor for features) and keep their numbers; the refined wording applies from `v0.4.1`. `v0.4.1` changes the default behavior of `auth login` and `auth setup` (the browser now opens in non-agent shells) and adds one hint line in agent shells; that is why its history row, written after the release, says so. The first release is `v0.1.0`; `v0.2.0` (the first with `gdcli update`) is the third one. All releases are listed in Release history.
- Never force-push a branch. Never move, rewrite or delete a tag whose release was published. A tag whose run failed before the publish call (no release, or a draft) was never usable and may be deleted and re-created; this is the only permitted tag deletion.

Procedure (nothing is published by hand: GoReleaser publishes the draft at the end of the run):

1. Set `version` in `Cargo.toml`, then run `cargo check` so cargo updates `Cargo.lock` (never edit `Cargo.lock` by hand).
2. Commit (after the independent review passed) and push to `main`.
3. Wait for CI to be green on that commit: `gh run list --workflow ci.yml --commit "$(git rev-parse HEAD)" --limit 1`, then `gh run watch <run-id> --exit-status`. If the list is empty right after the push, repeat the command.
4. Create the annotated tag on that commit: `git tag -a vX.Y.Z -m vX.Y.Z`.
5. Push it: `git push origin vX.Y.Z`.
6. Watch the release run: `gh run list --workflow release.yml --branch vX.Y.Z --limit 1` (a tag-triggered run reports the tag name as its branch; if the list is empty right after the push, repeat the command), then `gh run watch <run-id> --exit-status`.
7. Verify (checklist below). The tag run itself is the test of the pipeline.

Verification checklist, in a clean directory (`cd "$(mktemp -d)"`), with `vX.Y.Z` as the tag:

1. Release state and assets: `gh release view vX.Y.Z --repo pycabbage/gdcli --json tagName,isDraft,isPrerelease,isImmutable,assets --jq '{tagName,isDraft,isPrerelease,isImmutable,assets:[.assets[].name]}'`. Expected: `isDraft` false, `isPrerelease` false, exactly the assets `gdcli_checksums.txt`, `gdcli_linux_aarch64.tar.gz`, `gdcli_linux_x86_64.tar.gz`. `isImmutable` is true only when the repository setting is enabled (the setting itself needs admin access to read).
2. Download everything: `gh release download vX.Y.Z --repo pycabbage/gdcli`.
3. Checksums: `sha256sum -c gdcli_checksums.txt` (both archives OK).
4. Build provenance attestation: `gh attestation verify gdcli_linux_$(uname -m).tar.gz --repo pycabbage/gdcli`.
5. Release attestation, meaningful when the release is immutable: `gh release verify vX.Y.Z --repo pycabbage/gdcli` and `gh release verify-asset vX.Y.Z gdcli_linux_$(uname -m).tar.gz --repo pycabbage/gdcli`.
6. Binary: `tar -xzf gdcli_linux_$(uname -m).tar.gz && ./gdcli --version && ./gdcli --help`. The version must print `X.Y.Z`; the archive must contain only `gdcli` at its root (`tar -tzf`).
7. README install path in a clean home: `HOME="$(mktemp -d)" bash -c 'curl -fsSL -o gdcli.tar.gz https://github.com/pycabbage/gdcli/releases/latest/download/gdcli_linux_$(uname -m).tar.gz && mkdir -p ~/.local/bin && tar -xzf gdcli.tar.gz -C ~/.local/bin gdcli && ~/.local/bin/gdcli --version'` (the README's first two lines, plus a version check instead of the interactive `gdcli auth setup`). This also proves that `releases/latest/download` resolves to the new release.
8. Self-update against the real release (needs `gh` and a Rust toolchain; run as a normal user, with `REPO` set to the checkout to test and, optionally, `CARGO_TARGET_DIR` to keep the build cache between runs; the script creates its own scratch directory `$root` with `mktemp -d`, works only below `$root` (and the optional target directory), deletes `$root` at the end only after checking that it owns it, never touches the repository, `~/.local/bin` (it is only hashed before and after) or Google credentials, and runs `gdcli` with `HOME` and the XDG variables pointing into `$root`; if `/tmp` is mounted `noexec`, set `TMPDIR` to a directory that allows executing files): the script below copies `Cargo.toml`, `Cargo.lock`, `src`, `docs` and `skills` (`include_str!` embeds `docs/setup.md` and the files of `skills/gdcli`, so the build fails without them) and builds three scratch copies of the current tree with the versions 0.0.1, the latest released version and 9.0.0, installs the 0.0.1 build, runs `update` and checks the printed lines, `--version`, the API digest against the sha256 of the asset and of the installed file, the mode and owner, the leftovers, an equal version, a newer-than-release version, an install through a symlink, a 0700 file, a read-only directory, the rejection of an invalid `GITHUB_TOKEN` (the variable is read and sent to `api.github.com`: GitHub answers 401 and the exact message `github rejected GITHUB_TOKEN (401): unset it or provide a valid token` is printed with the binary unchanged), an empty `GITHUB_TOKEN` (treated as no token: a normal update) and that nothing else was written. The helper `isolated` removes any `GITHUB_TOKEN` of the caller from the environment of `gdcli`, so an ambient token never changes the other cases. Before a release that contains `update` the installed release has no `update` command, so the second run is skipped; after the release the script also runs the installed release binary's own `update` and expects `status: up_to_date`. On failure `$root` is kept (its permissions restored) and printed at the start for inspection. Rate limit: the block runs `gdcli update` without a token nine times (the plain, second, equal-version, newer, symlink, 0700 and empty-token runs and the two read-only runs), one anonymous API request each, against the anonymous limit of 60 requests per hour per IP; the invalid-token run sends a token and the `gh api` calls use the caller's `gh` login. Nine requests per run against 60 per hour mean that the first six runs within one hour fit and a seventh fails with the anonymous rate-limit message (`error: github api rate limit exceeded; resets at <time>; set GITHUB_TOKEN to raise the limit of 60 requests per hour`), or earlier when other anonymous requests share the IP. The caller may export `GITHUB_TOKEN` in their own shell for other commands, but the block removes it on purpose from the isolated cases (`isolated` runs `env -u GITHUB_TOKEN`; the two token cases set it explicitly), so exporting it does not raise the limit of the block.

```
set -eu
[ "$(id -u)" != 0 ] || { echo "run as non-root"; exit 1; }
repo=${REPO:?set REPO to the checkout}
root=$(mktemp -d)
target=${CARGO_TARGET_DIR:-$root/target}
trap 'chmod -R u+w "$root" 2>/dev/null || true' EXIT
echo "scratch directory: $root"
mkdir -p "$root/src" "$root/bins" "$root/dl" "$root/home"
cp -r "$repo/Cargo.toml" "$repo/Cargo.lock" "$repo/src" "$repo/docs" "$repo/skills" "$root/src/"
ensure() {
    "$@" || { echo "FAILED: $*"; exit 1; }
}
expect() {
    printf '%s\n' "$1" | grep -qxF "$2" || { echo "MISSING LINE: $2"; exit 1; }
    echo "ok: $2"
}
isolated() {
    env -u GITHUB_TOKEN HOME="$root/home" XDG_CONFIG_HOME="$root/home/config" XDG_DATA_HOME="$root/home/data" "$@"
}
build() {
    sed -i "0,/^version = \".*\"/s//version = \"$1\"/" "$root/src/Cargo.toml"
    sed -n 1,3p "$root/src/Cargo.toml"
    CARGO_TARGET_DIR="$target" cargo build --release --manifest-path "$root/src/Cargo.toml"
    cp "$target/release/gdcli" "$root/bins/$1"
    "$root/bins/$1" --version
}
stage() {
    mkdir -p "$root/$1"
    cp "$root/bins/$2" "$root/$1/gdcli"
    chmod 775 "$root/$1/gdcli"
}
latest=$(gh api repos/pycabbage/gdcli/releases/latest --jq .tag_name)
version=${latest#v}
asset=gdcli_linux_$(uname -m).tar.gz
echo "latest release: $latest"
build 0.0.1
build "$version"
build 9.0.0
gh release download "$latest" --repo pycabbage/gdcli --pattern "$asset" --dir "$root/dl"
local_bin=$HOME/.local/bin/gdcli
before_local=$(sha256sum "$local_bin" 2>/dev/null || echo none)

echo "--- plain: 0.0.1 -> $version"
stage plain 0.0.1
out=$(isolated /usr/bin/time -f "maxrss=%M KB" -o "$root/rss.txt" "$root/plain/gdcli" update)
echo "$out"
cat "$root/rss.txt"
expect "$out" "status: updated"
expect "$out" "current: 0.0.1"
expect "$out" "latest: $version"
expect "$("$root/plain/gdcli" --version)" "gdcli $version"
api_digest=$(gh api repos/pycabbage/gdcli/releases/latest --jq ".assets[] | select(.name==\"$asset\") | .digest")
asset_digest=sha256:$(sha256sum "$root/dl/$asset" | cut -d' ' -f1)
installed=$(sha256sum "$root/plain/gdcli" | cut -d' ' -f1)
entry=$(tar -xzOf "$root/dl/$asset" gdcli | sha256sum | cut -d' ' -f1)
echo "api digest:    $api_digest"
echo "asset sha256:  $asset_digest"
echo "installed:     $installed"
echo "archive entry: $entry"
ensure [ "$api_digest" = "$asset_digest" ]
ensure [ "$installed" = "$entry" ]
echo "digests ok"
ensure [ "$(stat -c '%a %U:%G' "$root/plain/gdcli")" = "775 $(id -un):$(id -gn)" ]; echo "mode and owner ok: 775 $(id -un):$(id -gn)"
ensure [ "$(ls -A "$root/plain")" = gdcli ]; echo "no leftovers: only gdcli"
if "$root/plain/gdcli" --help | grep -q update; then
    second=$(isolated "$root/plain/gdcli" update)
    echo "$second"
    expect "$second" "status: up_to_date"
    expect "$second" "current: $version"
    expect "$second" "latest: $version"
else
    echo "installed release has no update command yet: second run skipped"
fi

echo "--- equal version $version"
stage same "$version"
sha256sum "$root/same/gdcli" > "$root/same.sha"
out=$(isolated "$root/same/gdcli" update)
echo "$out"
expect "$out" "status: up_to_date"
expect "$out" "current: $version"
expect "$out" "latest: $version"
sha256sum -c "$root/same.sha"
ensure [ "$(ls -A "$root/same")" = gdcli ]; echo "no leftovers: only gdcli"

echo "--- newer than the release: 9.0.0"
stage newer 9.0.0
out=$(isolated "$root/newer/gdcli" update)
echo "$out"
expect "$out" "status: up_to_date"
expect "$out" "current: 9.0.0"
expect "$out" "latest: $version"

echo "--- symlink"
mkdir -p "$root/link/real"
cp "$root/bins/0.0.1" "$root/link/real/gdcli"
chmod 775 "$root/link/real/gdcli"
ln -s real/gdcli "$root/link/gdcli"
out=$(isolated "$root/link/gdcli" update)
echo "$out"
expect "$out" "status: updated"
ensure [ -L "$root/link/gdcli" ]; echo "link kept"
expect "$("$root/link/real/gdcli" --version)" "gdcli $version"
echo "listing: $(ls -A "$root/link" "$root/link/real" | tr '\n' ' ')"

echo "--- mode 700 stays 700"
stage private 0.0.1
chmod 700 "$root/private/gdcli"
out=$(isolated "$root/private/gdcli" update)
expect "$out" "status: updated"
ensure [ "$(stat -c %a "$root/private/gdcli")" = 700 ]; echo "mode kept: 700"

echo "--- read-only directory"
stage readonly 0.0.1
chmod 555 "$root/readonly"
before=$(sha256sum "$root/readonly/gdcli")
if isolated "$root/readonly/gdcli" update; then echo UNEXPECTED-SUCCESS; exit 1; fi
message=$(isolated "$root/readonly/gdcli" update 2>&1 || true)
expect "$message" "error: cannot write to $root/readonly: Permission denied (os error 13)"
ensure [ "$before" = "$(sha256sum "$root/readonly/gdcli")" ]; echo "binary unchanged"
chmod 755 "$root/readonly"
ensure [ "$(ls -A "$root/readonly")" = gdcli ]; echo "no leftovers: only gdcli"

echo "--- invalid GITHUB_TOKEN is sent to the API and rejected"
stage badtoken 0.0.1
before=$(sha256sum "$root/badtoken/gdcli")
message=$(isolated env GITHUB_TOKEN=gdcli-e2e-invalid-token "$root/badtoken/gdcli" update 2>&1 || true)
expect "$message" "error: github rejected GITHUB_TOKEN (401): unset it or provide a valid token"
ensure [ "$before" = "$(sha256sum "$root/badtoken/gdcli")" ]; echo "binary unchanged"
expect "$("$root/badtoken/gdcli" --version)" "gdcli 0.0.1"
ensure [ "$(ls -A "$root/badtoken")" = gdcli ]; echo "no leftovers: only gdcli"

echo "--- empty GITHUB_TOKEN means no token"
stage emptytoken 0.0.1
out=$(isolated env GITHUB_TOKEN= "$root/emptytoken/gdcli" update)
echo "$out"
expect "$out" "status: updated"
expect "$("$root/emptytoken/gdcli" --version)" "gdcli $version"
ensure [ "$(ls -A "$root/emptytoken")" = gdcli ]; echo "no leftovers: only gdcli"

echo "--- nothing else touched"
ensure [ -z "$(ls -A "$root/home")" ]; echo "isolated HOME and XDG directories stayed empty"
ensure [ "$before_local" = "$(sha256sum "$local_bin" 2>/dev/null || echo none)" ]; echo "~/.local/bin/gdcli unchanged"
ensure [ -O "$root" ]
chmod -R u+w "$root"
rm -rf "$root"
echo E2E-OK
```

9. The embedded skill equals the tagged files (the released binary runs only as `env -i HOME=<scratch>/home XDG_CONFIG_HOME=<scratch>/config XDG_DATA_HOME=<scratch>/data PATH="$PATH" ./gdcli ...`, see `testing.md`): `gdcli skills | cmp - <(git show vX.Y.Z:skills/gdcli/SKILL.md)`, the same for each topic against `skills/gdcli/references/<topic>.md`, and `gdcli skills full | wc -c` equals the five file sizes plus 4.
10. The skill installs from the repository (`skills.md`): `npx -y skills@1.9.0 add pycabbage/gdcli --list` prints `Found 1 skill` and `gdcli`, and `add pycabbage/gdcli --skill gdcli -a claude-code -y` in an empty project directory (with `HOME` and the npm cache in a scratch directory) installs `SKILL.md` and `references/*.md` byte for byte equal to the tag. On a host without `npx` (the `v0.4.0` check ran on one) `npm exec --yes --package=skills@1.9.0 -- skills add pycabbage/gdcli --list` and the same `add` arguments do the same; `skills-lock.json` of the project then records the commit of the tag. Run this step before any later commit changes `skills/`; otherwise compare with `git show <commit recorded in skills-lock.json>:skills/...`.

Failure and recovery:

| Situation | Action |
|---|---|
| The tag run failed before publishing (any step: tests, build, upload); the release, if any, is still a draft | Delete the remote and local tag: `git push --delete origin vX.Y.Z` and `git tag -d vX.Y.Z`. Delete the leftover draft only if `gh release view vX.Y.Z --repo pycabbage/gdcli --json isDraft --jq .isDraft` prints `true`: `gh release delete vX.Y.Z --repo pycabbage/gdcli --yes`. Fix the cause, commit, push, wait for CI, then tag the same version again. |
| The release was published (immutable or not) but something is wrong (bad asset, wrong version, any failed verification) | Never reuse that version or tag, and never edit or delete its assets. Fix the cause, bump the patch version in `Cargo.toml`, and release the next patch with the normal procedure. |
| The attest step failed after GoReleaser published | The release is already published: same as the previous row (ship the next patch). A published release without a provenance attestation is not repaired in place. |
| The tag was pushed on the wrong commit before any publish | Same as the first row, then tag the right commit. |

Only a release that was already published burns its version. Before the publish call nothing is public or locked, so deleting that unusable tag and its draft and tagging again is safe. The tag rules above apply throughout: no force-push of branches, and a tag whose release was published is never moved, rewritten or deleted.

## Repository requirements

- GitHub Actions enabled; workflow permissions may stay at the read-only default, since `release.yml` requests `contents: write`, `id-token: write` and `attestations: write` itself.
- No secrets or variables to configure.
- Verified on GitHub by the tag runs (v0.1.0: run 44239711584, with the earlier workflow that used `mlugg/setup-zig` and `astral-sh/setup-uv`; v0.1.1: run 44242336899, with the current workflow, success): `pip install cargo-zigbuild` on the runner (user-site fallback, then GoReleaser built x86_64 and aarch64), the draft, upload and publish flow, both releases immutable and passing `gh release verify`, and the attest step (`actions/attest-build-provenance@v4` ran `actions/attest@508db95...`: `Attestation created for 2 subjects`, signed with the Public Good Sigstore instance, signature uploaded to the Rekor transparency log, `Attestation uploaded to repository`).
- Verified on GitHub for `v0.2.0`: feature commit `4c0540a` (CI run 44450189666, success), release commit `7e87936` (CI run 44451059072, success), annotated tag object `26b5786` pointing to `7e87936`, release run 44451262814 (success in 3 min 4 s on image `ubuntu-26.04` 20261111.82.6, rustc 1.100.0, GoReleaser 2.20.2, cargo-zigbuild 0.24.1, ziglang 0.17.1; `Attestation created for 2 subjects`, attestation 69472097, no annotations). The release was published at 2026-11-19T03:27:54Z with `isImmutable: true`, `isPrerelease: false` and exactly the three assets (`gdcli_checksums.txt` 185 bytes, `gdcli_linux_aarch64.tar.gz` 1,778,208 bytes, `gdcli_linux_x86_64.tar.gz` 1,862,914 bytes). `sha256sum -c gdcli_checksums.txt`, `gh attestation verify` for both archives, `gh release verify v0.2.0` and `gh release verify-asset` for both archives pass; the binaries inside (x86_64 3,768,576 bytes, aarch64 3,260,096 bytes) need at most `GLIBC_2.30`; the README install snippet in a clean home installs `gdcli 0.2.0`.
- Verified against the genuine `v0.2.0` release: the self-update script of verification step 8 printed `E2E-OK` on 2026-11-19; results in docs/design/update.md.
- Verified on GitHub for `v0.3.0`: feature commit `ac3a51c` (CI run 44653584306, success), release commit `8aee421` (CI run 44653758728, success), annotated tag object `b040229` pointing to `8aee421`, release run 44653940829 (success, job 2 min 50 s, image `ubuntu-26.04` 20261111.196.1, rustc 1.100.0, GoReleaser 2.20.2, cargo-zigbuild 0.24.1, ziglang 0.17.1; `Attestation created for 2 subjects`, attestation 69941909, no annotations). The release was published at 2026-11-20T01:31:49Z with `isImmutable: true`, `isPrerelease: false` and exactly the three assets (`gdcli_checksums.txt` 185 bytes, `gdcli_linux_aarch64.tar.gz` 1,781,669 bytes, `gdcli_linux_x86_64.tar.gz` 1,865,410 bytes). `sha256sum -c gdcli_checksums.txt`, `gh attestation verify` for both archives, `gh release verify v0.3.0` and `gh release verify-asset` for both archives pass; the binaries inside (x86_64 3,772,528 bytes, aarch64 3,268,400 bytes) need at most `GLIBC_2.30`; the README install snippet in a clean home installs `gdcli 0.3.0`. Sizes against `v0.2.0`: `performance.md`.
- Verified against the genuine `v0.3.0` release: the self-update script of verification step 8 printed `E2E-OK` on 2026-11-20, including the invalid-token and the empty-token cases; the token runs of the released binary are in docs/design/update.md.
- Verified on GitHub for `v0.4.0`: work package commits `b9ec182` (accounts, CI run 44675217084), `f5284fd` (scenario features, CI run 44682066962) and `e06045d` (skills, CI run 44688583605), release commit `ab9368c` (CI run 44690039819), all success; annotated tag object `5c23f2b` pointing to `ab9368c`; release run 44690252902 (success, job 2 min 53 s, image `ubuntu-26.04` 20261111.196.1, rustc 1.100.0, GoReleaser v2.20.3, cargo-zigbuild 0.24.1, ziglang 0.17.1; `Attestation created for 2 subjects`, attestation 70020036, no annotations). The release was published at 2026-11-20T06:49:59Z with `isImmutable: true`, `isPrerelease: false` and exactly the three assets (`gdcli_checksums.txt` 185 bytes, `gdcli_linux_aarch64.tar.gz` 1,834,485 bytes, `gdcli_linux_x86_64.tar.gz` 1,920,454 bytes); each archive holds only `gdcli` at its root (the skill is embedded in the binary, `skills/` is not shipped). `sha256sum -c gdcli_checksums.txt`, the API digests, `gh attestation verify` for both archives (certificate workflow `release.yml@refs/tags/v0.4.0`, source digest `ab9368c`), `gh release verify v0.4.0` and `gh release verify-asset` for both archives pass; the binaries inside (x86_64 3,902,736 bytes, aarch64 3,378,208 bytes) need at most `GLIBC_2.30`; the README install snippet in a clean home installs `gdcli 0.4.0`. Sizes against `v0.3.0`: `performance.md`.
- Verified against the genuine `v0.4.0` release: the self-update script of verification step 8 printed `E2E-OK` on 2026-11-20 (peak RSS 6,648 KB in the first run and 6,400 KB in the second; two runs of the block on 2026-11-20; results in docs/design/update.md); step 9: the released binary run with an empty environment prints `SKILL.md` and each reference byte for byte equal to the files at the tag, and `gdcli skills full` is 19,634 bytes (19,630 plus 4); step 10 with the `npm exec` form: `Found 1 skill` (`gdcli`), and the install into an empty project is byte equal to the tag (`skills-lock.json` records `ab9368c`). Real-credential passes on copies of the installed credentials (counts only, `testing.md`): the release candidate passed R1 to R8 and the shipped `v0.4.0` binary (the downloaded asset, sha256 equal to the API digest) passed V1 to V6 on 2026-11-20.
- Still unverified: the aarch64 binaries were only inspected (`file`, dynamic symbol versions) and never executed (no aarch64 host); the first real prerelease tag (`prerelease: auto` is proven against the GoReleaser source and a mock API only); the repository immutability setting itself cannot be read with the bot token (`gh api repos/pycabbage/gdcli/immutable-releases` returns 403), although the releases report `isImmutable: true`; `gdcli update` on a root-owned or package-managed install path (only a read-only directory is covered); `GITHUB_TOKEN` as provided by Actions, fine-grained personal access tokens and classic personal access tokens (only a GitHub App installation token was exercised); the checks that need a human or a second account (the browser login of a second account, the interactive `auth setup`, a meeting with several transcript or notes sessions, a non-English account, a participant who is not the organizer) and `smartNotes.list` for a meeting without Gemini notes (`testing.md`); future runner image changes (`ubuntu-latest` is `ubuntu-26.04` since November 2026, see actions/runner-images issue 14748), which may change the Python, pip or `PATH` setup that `pip install cargo-zigbuild` relies on.
- The lockfile and workflows must be committed together.
- Release procedure: see Release management. The release workflow creates a draft release, attaches the assets, publishes it, then attests the archives.

## Release history

| Tag | Commit | Run | Notes |
|---|---|---|---|
| `v0.1.0` | `9f64bb2` | 44239711584 | First release; run with the earlier workflow (`mlugg/setup-zig`, `astral-sh/setup-uv`). |
| `v0.1.1` | `6443451` | 44242336899 | Verifies the workflow after the star-rule change (`pip install cargo-zigbuild`, `actions/attest-build-provenance@v4`); no functional change. |
| `v0.2.0` | `7e87936` | 44451262814 | First release with `gdcli update`; `prerelease: auto` added to `.goreleaser.yaml`; feature commit `4c0540a`; verified end to end (assets, digests, attestation, `gh release verify`, README install, real self-update run). |
| `v0.3.0` | `8aee421` | 44653940829 | `GITHUB_TOKEN` support in `gdcli update`; the `update` module split into `src/update/mod.rs` and `src/update/fetch.rs`; feature commit `ac3a51c`; verified end to end (assets, digests, attestation, `gh release verify`, README install, real self-update run with invalid, valid and empty tokens). |
| `v0.4.0` | `ab9368c` | 44690252902 | First release with accounts (the `auth` group and `--account`), the scenario features (`meet notes`, `meet transcript`, attachments, range mode and `--query`) and the agent skill (`gdcli skills`); breaking: `setup` and `login` moved under `auth`; work package commits `b9ec182`, `f5284fd`, `e06045d`; GoReleaser v2.20.3 (`~> v2`); verified end to end (assets, digests, attestation, `gh release verify`, README install, skill equality and install, real self-update run, real-credential passes). |
