mod common;

use clap::{CommandFactory, Parser, ValueEnum};
use common::{EVENTS_PATH, Harness, mount_file, mount_full_meeting, mount_tabs, tab_tree};
use gdcli::cli::Topic;
use gdcli::{Cli, calendar, docs, meet};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

const REFERENCES: [&str; 4] = ["meet", "calendar", "docs", "auth"];

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("skills/gdcli")
}

fn skill() -> String {
    std::fs::read_to_string(root().join("SKILL.md")).unwrap()
}

fn reference(name: &str) -> String {
    std::fs::read_to_string(root().join(format!("references/{name}.md"))).unwrap()
}

fn files() -> Vec<(String, String)> {
    let mut files = vec![("SKILL.md".to_owned(), skill())];
    files.extend(
        REFERENCES
            .iter()
            .map(|name| (format!("{name}.md"), reference(name))),
    );
    files
}

fn gdcli(args: &[&str]) -> Output {
    let home = tempfile::tempdir().unwrap();
    Command::new(env!("CARGO_BIN_EXE_gdcli"))
        .args(args)
        .env_clear()
        .env("HOME", home.path())
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

fn leaves(
    command: &clap::Command,
    path: &mut Vec<String>,
    found: &mut Vec<(String, clap::Command)>,
) {
    for sub in command
        .get_subcommands()
        .filter(|sub| sub.get_name() != "help")
    {
        path.push(sub.get_name().to_owned());
        if sub.has_subcommands() {
            leaves(sub, path, found);
        } else {
            found.push((path.join(" "), sub.clone()));
        }
        path.pop();
    }
}

fn all_leaves() -> Vec<(String, clap::Command)> {
    let mut found = Vec::new();
    leaves(&Cli::command(), &mut Vec::new(), &mut found);
    found
}

fn documented_leaves() -> Vec<(String, clap::Command)> {
    all_leaves()
        .into_iter()
        .filter(|(path, _)| path.contains(' '))
        .collect()
}

fn group(path: &str) -> &str {
    path.split(' ').next().unwrap()
}

fn section(text: &str, path: &str) -> Option<String> {
    let heading = format!("## gdcli {path}");
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.iter().position(|line| *line == heading)?;
    let body: Vec<&str> = lines[start + 1..]
        .iter()
        .copied()
        .take_while(|line| !line.starts_with("## "))
        .collect();
    Some(body.join("\n"))
}

fn usage_tokens(path: &str) -> BTreeSet<String> {
    let body = section(&reference(group(path)), path).unwrap();
    let line = body.lines().find(|line| !line.is_empty()).unwrap();
    assert!(
        line == format!("`gdcli {path}`") || line.starts_with(&format!("`gdcli {path} ")),
        "{path}: {line}"
    );
    line.split(|c: char| c.is_whitespace() || "[]|,=`".contains(c))
        .filter(|token| !token.is_empty())
        .map(str::to_owned)
        .collect()
}

fn options(command: &clap::Command) -> Vec<&clap::Arg> {
    command
        .get_arguments()
        .filter(|arg| !arg.is_positional() && !matches!(arg.get_id().as_str(), "help" | "version"))
        .collect()
}

#[test]
fn skill_and_reference_outputs_equal_the_files() {
    let output = gdcli(&["skills"]);
    assert!(output.status.success());
    assert_eq!(output.stdout, skill().into_bytes());
    assert_eq!(output.stderr, b"");
    for name in REFERENCES {
        let output = gdcli(&["skills", name]);
        assert!(output.status.success(), "{name}");
        assert_eq!(output.stdout, reference(name).into_bytes(), "{name}");
        assert_eq!(output.stderr, b"", "{name}");
    }
}

#[test]
fn full_is_the_five_files_joined_by_one_newline() {
    let expected: Vec<String> = files().into_iter().map(|(_, text)| text).collect();
    let output = gdcli(&["skills", "full"]);
    assert!(output.status.success());
    assert_eq!(output.stdout, expected.join("\n").into_bytes());
    assert_eq!(output.stderr, b"");
}

#[test]
fn unknown_topic_exits_2() {
    for topic in ["bogus", "Meet", "SKILL", ""] {
        let output = gdcli(&["skills", topic]);
        assert_eq!(output.status.code(), Some(2), "{topic:?}");
        assert_eq!(output.stdout, b"", "{topic:?}");
        assert!(
            String::from_utf8_lossy(&output.stderr)
                .contains("possible values: full, meet, calendar, docs, auth"),
            "{topic:?}"
        );
    }
    for args in [&["skills", "meet", "docs"][..], &["skills", "--json"]] {
        assert_eq!(gdcli(args).status.code(), Some(2), "{args:?}");
    }
}

#[test]
fn frontmatter_is_name_and_description_only() {
    let text = skill();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[0], "---");
    let end = lines[1..].iter().position(|line| *line == "---").unwrap() + 1;
    let front = &lines[1..end];
    assert_eq!(front.len(), 2, "{front:?}");
    let directory = root();
    let name = directory.file_name().unwrap().to_str().unwrap();
    assert_eq!(name, env!("CARGO_PKG_NAME"));
    assert_eq!(front[0], format!("name: {name}"));
    let description = front[1].strip_prefix("description: ").unwrap();
    assert!((1..=1024).contains(&description.chars().count()));
    assert!(!description.contains(": ") && !description.contains(" #"));
    assert!(!description.starts_with(|c: char| !c.is_ascii_alphanumeric()));
    assert!(!description.ends_with(':'));
    assert!(lines[end + 1..].iter().all(|line| !line.starts_with("---")));
}

#[test]
fn files_have_no_comments_and_end_with_one_newline() {
    for (name, text) in files() {
        assert!(text.is_ascii(), "{name}");
        assert!(!text.contains('\t') && !text.contains('\r'), "{name}");
        assert!(!text.contains("<!--"), "{name}");
        assert!(text.ends_with('\n') && !text.ends_with("\n\n"), "{name}");
        let mut fenced = false;
        for line in text.lines() {
            if line.starts_with("```") {
                fenced = !fenced;
            }
            assert!(!(fenced && line.starts_with('#')), "{name}: {line}");
        }
        assert!(!fenced, "{name}");
    }
}

#[test]
fn every_leaf_command_has_a_reference_section() {
    let documented = documented_leaves();
    assert_eq!(documented.len(), 12);
    for (path, _) in &documented {
        let text = reference(group(path));
        let body = section(&text, path).unwrap_or_else(|| panic!("{path}: no section"));
        let usage = body.lines().find(|line| !line.is_empty()).unwrap();
        assert!(
            usage == format!("`gdcli {path}`") || usage.starts_with(&format!("`gdcli {path} ")),
            "{path}: {usage}"
        );
    }
    let exempt: Vec<String> = all_leaves()
        .into_iter()
        .map(|(path, _)| path)
        .filter(|path| !path.contains(' '))
        .collect();
    assert_eq!(exempt, ["skills", "update"]);
}

#[test]
fn usage_lines_list_every_option_as_whole_tokens() {
    for (path, command) in documented_leaves() {
        let tokens = usage_tokens(&path);
        let mut known = BTreeSet::new();
        for arg in options(&command) {
            let long = arg.get_long().map(|long| format!("--{long}"));
            let short = arg.get_short().map(|short| format!("-{short}"));
            for token in long.iter().chain(&short) {
                assert!(tokens.contains(token), "{path}: {token} is not a token");
                known.insert(token.clone());
            }
            known.extend(
                arg.get_all_aliases()
                    .into_iter()
                    .flatten()
                    .map(|alias| format!("--{alias}")),
            );
        }
        for token in tokens.iter().filter(|token| token.starts_with('-')) {
            assert!(known.contains(token), "{path}: stale option {token}");
        }
        let has_json = options(&command)
            .iter()
            .any(|arg| arg.get_long() == Some("json"));
        assert_eq!(tokens.contains("--json"), has_json, "{path}");
    }
}

#[test]
fn usage_lines_list_every_positional() {
    for (path, command) in documented_leaves() {
        let tokens = usage_tokens(&path);
        let expected: BTreeSet<String> = command
            .get_arguments()
            .filter(|arg| arg.is_positional())
            .map(|arg| format!("<{}>", arg.get_id().as_str().to_uppercase()))
            .collect();
        let found: BTreeSet<String> = tokens
            .into_iter()
            .filter(|token| token.starts_with('<'))
            .collect();
        assert_eq!(found, expected, "{path}");
    }
}

#[test]
fn no_reference_section_names_a_missing_command() {
    let known: Vec<String> = documented_leaves()
        .into_iter()
        .map(|(path, _)| path)
        .collect();
    let mut headings = 0;
    for name in REFERENCES {
        for line in reference(name).lines() {
            let Some(path) = line.strip_prefix("## gdcli ") else {
                continue;
            };
            headings += 1;
            assert!(known.iter().any(|known| known == path), "{path}");
            assert_eq!(group(path), name, "{path}");
        }
    }
    assert_eq!(headings, known.len());
}

fn first_span(line: &str) -> Option<String> {
    let start = line.find('`')?;
    let ticks = line[start..].chars().take_while(|c| *c == '`').count();
    let rest = &line[start + ticks..];
    let end = rest.find(&"`".repeat(ticks))?;
    Some(rest[..end].trim().to_owned())
}

fn error_spans(text: &str) -> Vec<String> {
    let mut spans = Vec::new();
    let mut listing = false;
    for line in text.lines() {
        if line == "## Errors" || (line.starts_with("Errors") && line.ends_with(':')) {
            listing = true;
        } else if listing && (line.starts_with("- ") || line.starts_with('|')) {
            spans.extend(first_span(line));
        } else if !(listing && line.is_empty()) {
            listing = false;
        }
    }
    spans
}

fn fragments(span: &str) -> Vec<String> {
    let mut marked = String::new();
    let mut rest = span;
    while let Some(next) = rest.chars().next() {
        if next == '<' && rest.contains('>') {
            rest = &rest[rest.find('>').unwrap() + 1..];
            marked.push('\0');
        } else if let Some(after) = rest.strip_prefix("...") {
            rest = after;
            marked.push('\0');
        } else if next.is_ascii_digit() {
            rest = rest.trim_start_matches(|c: char| c.is_ascii_digit());
            marked.push('\0');
        } else {
            marked.push(next);
            rest = &rest[next.len_utf8()..];
        }
    }
    marked
        .split('\0')
        .map(|part| part.trim_matches(|c: char| c.is_whitespace() || ":;,`".contains(c)))
        .filter(|part| part.len() >= 6)
        .map(str::to_owned)
        .collect()
}

fn sources() -> String {
    let mut text = String::new();
    let mut pending = vec![PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src")];
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else {
                text += &std::fs::read_to_string(path).unwrap();
            }
        }
    }
    let text = text.replace("\\\"", "\"");
    let notes = text.replace("{label}", "gemini notes document");
    let transcript = text.replace("{label}", "transcript");
    format!("{text}{notes}{transcript}")
}

#[test]
fn error_strings_in_the_files_exist_in_the_source() {
    let source = sources();
    let mut checked = 0;
    for (name, text) in files() {
        for span in error_spans(&text) {
            let parts = fragments(&span);
            assert!(!parts.is_empty(), "{name}: {span}");
            for part in parts {
                assert!(source.contains(&part), "{name}: {span}: {part:?}");
            }
            checked += 1;
        }
    }
    assert!(checked >= 25, "{checked}");
}

#[test]
fn skill_md_covers_update_skills_and_every_topic_and_stays_short() {
    let text = skill();
    assert!(text.lines().count() < 500);
    for command in Cli::command()
        .get_subcommands()
        .filter(|sub| sub.get_name() != "help")
    {
        let name = format!("gdcli {}", command.get_name());
        assert!(text.contains(&name), "{name}");
    }
    let row = text
        .lines()
        .find(|line| line.contains("`gdcli skills ["))
        .and_then(first_span)
        .unwrap();
    for topic in Topic::value_variants() {
        let name = topic.to_possible_value().unwrap().get_name().to_owned();
        assert!(whole_word(&row, &name), "{name}");
        assert!(
            name == "full" || text.contains(&format!("gdcli skills {name}")),
            "{name}"
        );
    }
}

#[test]
fn skills_runs_with_an_empty_environment() {
    let output = Command::new(env!("CARGO_BIN_EXE_gdcli"))
        .arg("skills")
        .env_clear()
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, skill().into_bytes());
    assert_eq!(output.stderr, b"");
}

#[tokio::test]
async fn skills_does_not_resolve_the_paths() {
    let calls = std::cell::Cell::new(0);
    let resolve = || {
        calls.set(calls.get() + 1);
        anyhow::bail!("no home")
    };
    let cli = Cli::try_parse_from(["gdcli", "skills", "docs"]).unwrap();
    let mut out = Vec::new();
    gdcli::run_with(cli, &resolve, &mut out).await.unwrap();
    assert_eq!(out, reference("docs").into_bytes());
    assert_eq!(calls.get(), 0);
    let cli = Cli::try_parse_from(["gdcli", "auth", "list"]).unwrap();
    let mut out = Vec::new();
    let error = gdcli::run_with(cli, &resolve, &mut out).await.unwrap_err();
    assert_eq!(gdcli::message(&error), "no home");
    assert_eq!(out, b"");
    assert_eq!(calls.get(), 1);
}

fn keys(value: &Value, found: &mut BTreeSet<String>) {
    match value {
        Value::Object(map) => {
            for (key, item) in map {
                found.insert(key.clone());
                keys(item, found);
            }
        }
        Value::Array(items) => items.iter().for_each(|item| keys(item, found)),
        _ => {}
    }
}

fn whole_word(text: &str, word: &str) -> bool {
    let identifier = |c: char| c.is_ascii_alphanumeric() || c == '_';
    text.match_indices(word).any(|(at, _)| {
        !text[..at].chars().next_back().is_some_and(identifier)
            && !text[at + word.len()..]
                .chars()
                .next()
                .is_some_and(identifier)
    })
}

fn assert_documented(path: &str, outputs: &[String]) {
    let mut found = BTreeSet::new();
    for output in outputs {
        keys(&serde_json::from_str(output).unwrap(), &mut found);
    }
    let body = section(&reference(group(path)), path).unwrap();
    for key in &found {
        assert!(
            whole_word(&body, key),
            "{path}: key {key} is not documented"
        );
    }
    assert!(found.len() > 15, "{path}: {found:?}");
}

#[tokio::test]
async fn calendar_get_keys_are_documented() {
    let harness = Harness::start().await;
    let mut event = common::calendar_event();
    event["attachments"] = common::calendar_attachments();
    harness
        .mount_json(&format!("{EVENTS_PATH}/abc123"), event)
        .await;
    let mut out = Vec::new();
    calendar::get(&harness.api, "abc123", true, &mut out)
        .await
        .unwrap();
    assert_documented("calendar get", &[common::text(out)]);
}

#[tokio::test]
async fn meet_get_keys_are_documented() {
    let mut outputs = Vec::new();
    for status in [200, 403] {
        let harness = Harness::start().await;
        mount_full_meeting(&harness, status).await;
        let mut out = Vec::new();
        meet::get(&harness.api, "evt1", true, &mut out)
            .await
            .unwrap();
        outputs.push(common::text(out));
    }
    assert_documented("meet get", &outputs);
}

#[tokio::test]
async fn docs_metadata_keys_are_documented() {
    let mut outputs = Vec::new();
    for failing in [false, true] {
        let harness = Harness::start().await;
        mount_file(&harness, "application/vnd.google-apps.document").await;
        if failing {
            harness
                .mount_status(
                    "/v1/documents/DOC123",
                    403,
                    json!({"error": {"code": 403, "message": "off"}}),
                )
                .await;
        } else {
            mount_tabs(&harness, tab_tree()).await;
        }
        let mut out = Vec::new();
        docs::metadata(&harness.api, "DOC123", true, &mut out)
            .await
            .unwrap();
        outputs.push(common::text(out));
    }
    assert_documented("docs metadata", &outputs);
}
