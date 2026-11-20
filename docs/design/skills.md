# Agent skill

`skills/gdcli/` is the agent skill of gdcli in the Agent Skills format (https://agentskills.io/specification): `SKILL.md` (about 55 lines, loaded when the skill is activated) and one reference per command group, loaded on demand. It is the single source of truth for how an agent uses gdcli, and the binary embeds it, so `gdcli skills` always prints the text that belongs to the installed version.

```
skills/gdcli/SKILL.md
skills/gdcli/references/meet.md
skills/gdcli/references/calendar.md
skills/gdcli/references/docs.md
skills/gdcli/references/auth.md
src/skills.rs
```

## Who owns what

| Information | Owner |
|---|---|
| Usage of every command that has a reference (options, value ranges, defaults, list columns, record keys and their order, output examples), the wording of its error messages and what to do about them, the agent workflow | `skills/gdcli/**`. Usage lines, option lists, column lists and record key lists are not repeated elsewhere in the repository (`AGENTS.md` keeps the one-line command list, the README keeps the install lines). A design document may quote a message only to explain the situation that produces it (`accounts.md`, `commands.md`, `auth.md`, `setup.md`); those quotes are not checked by a test, so a reworded message is changed in the reference first and then found with `grep -rn '<fragment>' docs` |
| Usage and messages of `update` (it has no reference; `SKILL.md` names the keys it prints) | `commands.md` and `update.md` |
| Rules shared by all commands (one model and two renderings, list style, `id` first, time handling, closed pipes, id arguments, where `--json` and `--account` exist, deadlines), the selection algorithms (windows, ring buffer, range semantics, query composition), the flows of `meet get`, `meet notes` and `docs get --tab`, observed facts, the `update` details and error table, the error text rules | `commands.md` |
| Account storage, selection, migration, atomicity, the Drive dependency, account messages | `accounts.md` |
| The OAuth client, scopes, login flows, refresh tokens | `auth.md` |
| The Google Cloud project, consent screen and upgrade steps for humans | `docs/setup.md` (also printed by `gdcli auth setup`) |
| Tests that keep the skill true to the program, and the agent rules for running a binary | `testing.md`, this file |

A change to a command, an option, a record key or an error message is finished when the matching reference says it: the contract tests below fail until it does. `skills` and `update` have no reference file; `SKILL.md` covers them (the command table, with the keys `update` prints). Every reference repeats the sentences it needs (the 730-day limit of `--query`, the range options), on purpose: each file can be loaded alone, so a pointer to another reference would cost an extra call.

## The command

`gdcli skills [full|meet|calendar|docs|auth]` (`cli::Topic`, a clap `ValueEnum`, lower case; the help shows `[possible values: full, meet, calendar, docs, auth]`):

| Argument | Output |
|---|---|
| none | `SKILL.md`, frontmatter included, byte for byte |
| `meet`, `calendar`, `docs`, `auth` | `references/<topic>.md` byte for byte |
| `full` | `SKILL.md`, then `meet.md`, `calendar.md`, `docs.md` and `auth.md`, with exactly one `\n` between the parts (one blank line, because every file ends with one newline) |

- Embedding: `src/skills.rs` holds five `include_str!("../skills/gdcli/...")` constants, so the files are part of the binary at build time. There are no copies and no generated duplicates in the repository; cargo tracks the files, so editing one triggers a rebuild (checked: `touch skills/gdcli/references/auth.md` made `cargo build -v` report `Dirty gdcli ... the file skills/gdcli/references/auth.md has changed` and recompile). The path is relative to `src/skills.rs`; every source copy must therefore contain `skills/` (CI and GoReleaser check out the repository; the self-update script of `release.md` copies `skills` next to `src` and `docs`, and the build fails loudly without it).
- `skills::print(topic, out)` writes the chosen parts in a loop (a `\n` before every part but the first); nothing is allocated, not even for `full`.
- The command needs no Google account, credentials, network or `--account`. `execute` in `lib.rs` (which takes the resolver and the output writer from `run_with(cli, resolve, out)`; `run` passes the locked stdout) handles `Command::Skills` and `Command::Update` before any call of `resolve`, so `gdcli skills` works on a machine without `HOME`, `XDG_*` or a passwd entry. It runs under the common 120 s deadline of `deadline_for` like every command except `auth setup` and `auth login`; writing a few kilobytes to a reader that never reads is the only way to reach it. A closed pipe ends it with exit status 0 like every other command.
- Not done on purpose: generating the text from the clap definition or from `commands.md` (the skill is hand-written for agents and the tests compare it with the clap tree instead), committed copies of the files, stripping the frontmatter (the output would differ from the file, and an agent that redirects `gdcli skills` into a skill directory gets a valid `SKILL.md`), `--json`, `--account`, a YAML crate for the frontmatter test.
- The installed copy of a skill can be older than the binary; `SKILL.md` says so and names `gdcli skills` as authoritative.

## File conventions

- `SKILL.md` frontmatter: exactly `name: gdcli` (equal to the directory name, 1 to 64 characters of `a-z0-9-`) and a one-line `description:` of 1 to 1024 characters that contains neither `: ` nor ` #` (the `skills` CLI parses real YAML) and starts with a letter or digit; no other keys. `SKILL.md` stays under 500 lines (it has about 55).
- ASCII only, LF only, no tab characters, one trailing newline, no HTML comments, no `#` line inside a code fence (a fence line would read as a heading to a naive parser). Table cells escape the pipe in a code span as `\|` (correct Markdown, so a renderer shows `gdcli meet list|get|notes|transcript`); the raw text that `gdcli skills` prints therefore contains the backslashes, and an agent that reads the raw text should ignore them.
- A reference has one `## gdcli <group> <command>` section per leaf command. The first non-empty line of the section is the usage line in backticks: `` `gdcli <path> [--name VALUE] ... [-a, --account EMAIL]` ``, every option in the form `[--name VALUE]` (short and long form of the account option written `[-a, --account EMAIL]`) and every positional as `<ID>` or `<EMAIL>`. Aliases (`--future`) appear in prose only. Value names in the usage line (`N`, `TIME`, `TEXT`) are free; the option names are matched as whole tokens.
- Error messages are written as the first code span of a bullet below a line `Errors ...:` (references) or of a table row below `## Errors` (`SKILL.md`), with `<name>` for a value and `...` for free text. Digits count as values. Messages that carry a label (`gemini notes document`, `transcript`) are written with the label spelled out.
- Records are documented as lists of keys in output order with the nested field names in parentheses; text and JSON use the same names (`commands.md`, two renderings).

## `npx skills` compatibility

Verified with `skills` 1.9.0 (vercel-labs/skills, commit `57217cf`): discovery looks in the repository root, `skills/`, `skills/.curated`, `.experimental`, `.system` and the agent skill directories; `name` and `description` must be strings; the installer copies the whole skill directory (only `metadata.json` and `.git` are excluded). `skills/gdcli/SKILL.md` is the only `SKILL.md` of the repository, because a second one would be listed as a second skill. Evidence of 2026-11-20 on a scratch copy of the final tree (HOME and the npm cache in a scratch directory, nothing installed outside it):

```
npx -y skills@1.9.0 add <copy> --list
Found 1 skill
gdcli  Read Google Calendar events, Google Meet meetings with their transcripts and Gemini notes, ...

npx -y skills@1.9.0 add <copy> --skill gdcli -a claude-code -y     (in an empty project directory)
gdcli (copied) -> ./.claude/skills/gdcli
cmp of SKILL.md, references/{meet,calendar,docs,auth}.md against the repository: identical; diff -rq of the two trees: identical
```

The GitHub shorthand `pycabbage/gdcli` cannot be tested before the push; the release checklist (`release.md`, step 10) runs `npx -y skills@1.9.0 add pycabbage/gdcli --list` and the install after the release.

## Tests

`tests/skills.rs` (16 tests; the binary tests start from `env_clear()` and a temporary `HOME`; the files are read at run time from `CARGO_MANIFEST_DIR/skills/gdcli`) and `tests/cli.rs` (`skills_takes_one_optional_topic_and_nothing_else`). They are the executable contract between the skill and the program; the clap tree is the oracle.

| Test | Pins |
|---|---|
| `skill_and_reference_outputs_equal_the_files` | `gdcli skills` and every topic print the file bytes (so a wrong constant or a stale build fails) |
| `full_is_the_five_files_joined_by_one_newline` | the order SKILL, meet, calendar, docs, auth and the separator |
| `unknown_topic_exits_2` | an unknown or differently cased topic, a second argument and `--json` are usage errors with the possible values listed |
| `frontmatter_is_name_and_description_only` | the frontmatter rules above, `name` equal to the directory and to the package name |
| `files_have_no_comments_and_end_with_one_newline` | ASCII, no tab or CR, no HTML comment, one trailing newline, no `#` line inside a fence |
| `every_leaf_command_has_a_reference_section` | each of the 12 nested leaves of `Cli::command()` (the generated `help` subcommand is skipped) has its section and usage line in the file of its group; only the top-level leaves `skills` and `update` are exempt |
| `usage_lines_list_every_option_as_whole_tokens` | every long and short option appears as a whole token of the usage line (so `-a` is not satisfied by `--account`), no token names an option that does not exist, `--json` is a token exactly when the command has it |
| `usage_lines_list_every_positional` | the positionals of the clap definition are exactly the `<...>` tokens |
| `no_reference_section_names_a_missing_command` | no stale `## gdcli ...` heading, every section sits in the file of its group |
| `error_strings_in_the_files_exist_in_the_source` | every error message in the error lists is cut at its placeholders and each remaining piece of at least 6 characters is found in `src/` (with the two read labels expanded) |
| `skill_md_covers_update_skills_and_every_topic_and_stays_short` | `gdcli <command>` for every top-level command, every topic name, under 500 lines |
| `skills_runs_with_an_empty_environment` | the binary without `HOME` and `XDG_*` prints `SKILL.md` (on a machine with a passwd entry this cannot detect a regression; the next test does) |
| `skills_does_not_resolve_the_paths` | `run_with` with a counting resolver that fails and a `Vec` as the writer: `skills docs` writes exactly the bytes of `references/docs.md` and never calls the resolver, `auth list` calls it once, fails with its error and writes nothing |
| `calendar_get_keys_are_documented`, `meet_get_keys_are_documented`, `docs_metadata_keys_are_documented` | every JSON key at any depth of a full record (the fixtures of `tests/common`; for `meet get` also the `meet_api` variant, for `docs metadata` the `tabs_unavailable` variant) is a whole identifier of the section of its command |

Mutation results of this package are in `testing.md`.

## Size

`v0.4.0` shipped five files of 19,630 bytes (225 lines; `gdcli skills full` prints 19,634 bytes). With the new command the local release binary grows by 37,096 bytes (`performance.md`, measured with the `0.3.0` manifest): 12,376 bytes of code (clap value enum, help strings, dispatch) and 24,720 bytes for the text including section alignment. The shipped-style local builds with the `0.4.0` manifest are 3,907,360 bytes (x86_64) and 3,382,832 bytes (aarch64), +3.57 % and +3.50 % against `v0.3.0`; the shipped `v0.4.0` binaries are 3,902,736 and 3,378,208 bytes.

After the release the tree carries a clarification in `SKILL.md` step 3 and in `references/meet.md` (the sections `meet notes` and `meet transcript`): a Gemini notes document has the notes tab and, when the meeting was transcribed into the same document, a second tab with the transcript; the `document_id` of the `transcript:` line equals that of the `smart_note:` line in that case and both commands print the same bytes without `--tab` (measurements: `commands.md`). The five files are then 20,528 bytes (`SKILL.md` 5,855, `meet.md` 6,648, `calendar.md` 3,102, `docs.md` 2,253, `auth.md` 2,670; still 225 lines) and `gdcli skills full` prints 20,532 bytes. `v0.4.0` shipped 19,630 bytes of skill text (19,634 for `full`); the next release carries the clarification.
