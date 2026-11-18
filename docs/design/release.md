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

- Trigger: `push` of tags matching `v*`. Tags are created and pushed by the PM agents (see Release management).
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
- GoReleaser version policy: `version: "~> v2"` is exactly what both the goreleaser-action README and the GoReleaser GitHub Actions docs show. The action resolves it to the newest v2.x, so GoReleaser updates arrive without edits. The config starts with `version: 2`, GoReleaser's own compatibility marker for the whole v2 line. No pin is used because no document recommends one. The config uses `release.preflight` (needs 2.18 or later) and `use_existing_draft` (2.5 or later); `goreleaser check` was run with v2.20.2, the newest v2 at the time of writing.
- No tool versions are pinned (`cargo-zigbuild` and the `ziglang` wheel come at their latest). If a new tool release ever breaks the release job, pin that tool at that point (`pip install cargo-zigbuild==X`).

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

- Source: https://goreleaser.com/customization/publish/attestations/ ("If you're using GitHub Actions and want to attest your build artifacts, you can add the following to your release workflow") and the `actions/attest` README. The GoReleaser page uses `actions/attest@v4`; this repository uses `actions/attest-build-provenance@v4` instead, because of the 1000-star rule (see Action policy).
- Implemented as documented: workflow permissions `id-token: write` and `attestations: write` (plus the `contents: write` GoReleaser needs), and `actions/attest-build-provenance@v4` after GoReleaser with `subject-checksums` (the input is passed through unchanged). The docs example path `./dist/checksums.txt` is adapted to our checksum file name `./dist/gdcli_checksums.txt` (`checksum.name_template` in `.goreleaser.yaml`). The checksum file is in the `sha256sum` format the action requires (digest, two spaces, file name).
- Accepted exception: `actions/attest-build-provenance@v4` (1077 stars) is a composite wrapper (since v4 "simply a wrapper on top of `actions/attest`"; its README says new implementations should use `actions/attest` directly) that runs `actions/attest@508db95...` (v4.2.1; 187 stars). The workflow names only the wrapper, which meets the 1000-star rule; the transitive `actions/attest` is the same GitHub-owned code the GoReleaser docs use directly, and `gh actions-lock` records it as a transitive dependency of the wrapper. The other candidates fail the rule as well (`slsa-framework/slsa-github-generator` 611 stars, `sigstore/cosign-installer` 214 stars) and `gh attestation` can only verify, not create. The PM decided to keep the wrapper.
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

Who: the PM agents manage tags and releases. The human owner pushes only exceptionally.

Tag policy:

- `vX.Y.Z`, annotated (`git tag -a`), always equal to `v` plus the `version` in `Cargo.toml`, so that `gdcli --version` (clap reads `CARGO_PKG_VERSION`) always matches the tag. The release workflow triggers on `v*`.
- SemVer: patch for fixes, minor for features. While the version is 0.x, breaking changes also bump minor. The first release is `v0.1.0`.
- Never force-push a branch. Never move, rewrite or delete a tag whose release was published. A tag whose run failed before the publish call (no release, or a draft) was never usable and may be deleted and re-created; this is the only permitted tag deletion.

Procedure (nothing is published by hand: GoReleaser publishes the draft at the end of the run):

1. Set `version` in `Cargo.toml`, then run `cargo check` so cargo updates `Cargo.lock` (never edit `Cargo.lock` by hand).
2. Commit (after the reviewer passed) and push to `main`.
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
7. README install path in a clean home: `HOME="$(mktemp -d)" bash -c 'curl -fsSL -o gdcli.tar.gz https://github.com/pycabbage/gdcli/releases/latest/download/gdcli_linux_$(uname -m).tar.gz && mkdir -p ~/.local/bin && tar -xzf gdcli.tar.gz -C ~/.local/bin gdcli && ~/.local/bin/gdcli --version'` (the README's first two lines, plus a version check instead of the interactive `gdcli setup`). This also proves that `releases/latest/download` resolves to the new release.

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
- Still unverified: the aarch64 binary was only inspected (`file`, `readelf`) and never executed; the repository immutability setting itself cannot be read with the bot token (`gh api repos/pycabbage/gdcli/immutable-releases` returns 403), although the releases report `isImmutable: true`; future runner image changes (`ubuntu-latest` moved to Ubuntu 26.04 in November 2026, see actions/runner-images issue 14748), which may change the Python, pip or `PATH` setup that `pip install cargo-zigbuild` relies on.
- The lockfile and workflows must be committed together.
- Release procedure: see Release management. The release workflow creates a draft release, attaches the assets, publishes it, then attests the archives.

## Release history

| Tag | Commit | Run | Notes |
|---|---|---|---|
| `v0.1.0` | `9f64bb2` | 44239711584 | First release; run with the earlier workflow (`mlugg/setup-zig`, `astral-sh/setup-uv`). |
| `v0.1.1` | `6443451` | 44242336899 | Verifies the workflow after the star-rule change (`pip install cargo-zigbuild`, `actions/attest-build-provenance@v4`); no functional change. |
