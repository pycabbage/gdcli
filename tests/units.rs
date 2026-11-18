use gdcli::docs::document_id;
use gdcli::events::{meeting_code, render_time, stamp, stamp_millis};
use gdcli::output::{field, flatten, row};
use gdcli::{Command, Paths, deadline_for};
use serde_json::json;
use std::io::{Error, ErrorKind};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

fn at(value: &str) -> OffsetDateTime {
    OffsetDateTime::parse(value, &Rfc3339).unwrap()
}

fn written(write: impl FnOnce(&mut Vec<u8>)) -> String {
    let mut out = Vec::new();
    write(&mut out);
    String::from_utf8(out).unwrap()
}

#[test]
fn document_ids_are_extracted_from_urls() {
    assert_eq!(document_id("1AbC_-x"), "1AbC_-x");
    for url in [
        "https://docs.google.com/document/d/1AbC_-x/edit",
        "https://docs.google.com/document/d/1AbC_-x/edit?tab=t.0",
        "https://docs.google.com/document/d/1AbC_-x",
        "https://docs.google.com/document/d/1AbC_-x#heading=h.1",
        "https://docs.google.com/document/d/1AbC_-x?usp=sharing",
        "https://docs.google.com/document/u/0/d/1AbC_-x/view?usp=sharing",
        "https://drive.google.com/file/d/1AbC_-x/view",
    ] {
        assert_eq!(document_id(url), "1AbC_-x", "{url}");
    }
}

#[test]
fn rows_are_tab_separated_with_control_characters_flattened() {
    let line = written(|out| row(out, &["a", "b\tc", "d\ne\r", ""]).unwrap());
    assert_eq!(line, "a\tb c\td e \t\n");
}

#[test]
fn fields_skip_empty_values_and_indent_continuations() {
    let lines = written(|out| {
        field(out, "empty", "  \n ").unwrap();
        field(out, "multi", "one\ntwo\n\nthree\n").unwrap();
    });
    assert_eq!(lines, "multi: one\n  two\n  \n  three\n");
}

#[test]
fn flatten_prints_scalars_with_dotted_keys_and_indexed_arrays() {
    let value = json!({"b": {"c": 1, "d": [true, null, "x"]}, "a": "y", "e": null, "f": {}});
    let lines = written(|out| flatten(out, "", &value).unwrap());
    assert_eq!(lines, "a: y\nb.c: 1\nb.d.0: true\nb.d.2: x\n");
}

#[test]
fn event_times_render_as_utc_seconds_or_dates() {
    let timed = json!({"start": {"dateTime": "2026-11-18T09:30:15+09:00"}});
    let all_day = json!({"start": {"date": "2026-11-20"}});
    assert_eq!(render_time(&timed, "start"), "2026-11-18T00:30:15Z");
    assert_eq!(render_time(&all_day, "start"), "2026-11-20");
    assert_eq!(render_time(&json!({"start": {}}), "start"), "");
    assert_eq!(render_time(&json!({}), "end"), "");
    assert_eq!(
        stamp(at("2026-11-18T09:30:15.75+09:00")),
        "2026-11-18T00:30:15Z"
    );
    assert_eq!(
        stamp_millis(at("2026-11-18T09:30:15+09:00")),
        "2026-11-18T00:30:15.000Z"
    );
}

#[test]
fn only_hangouts_meet_conferences_yield_a_code() {
    let meet = json!({"conferenceData": {"conferenceId": "abc-defg-hij", "conferenceSolution": {"key": {"type": "hangoutsMeet"}}}});
    let other = json!({"conferenceData": {"conferenceId": "123", "conferenceSolution": {"key": {"type": "addOn"}}}});
    let no_id =
        json!({"conferenceData": {"conferenceSolution": {"key": {"type": "hangoutsMeet"}}}});
    assert_eq!(meeting_code(&meet), Some("abc-defg-hij"));
    assert_eq!(meeting_code(&other), None);
    assert_eq!(meeting_code(&no_id), None);
    assert_eq!(meeting_code(&json!({})), None);
}

#[test]
fn paths_join_the_fixed_file_names() {
    let paths = Paths::new("/c/gdcli", "/d/gdcli");
    assert_eq!(
        paths.client_secret(),
        std::path::Path::new("/c/gdcli/client_secret.json")
    );
    assert_eq!(paths.token(), std::path::Path::new("/d/gdcli/token.json"));
    assert_eq!(paths.config, std::path::Path::new("/c/gdcli"));
    assert_eq!(paths.data, std::path::Path::new("/d/gdcli"));
}

#[derive(Debug)]
struct Wrapped(Error);

impl std::fmt::Display for Wrapped {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "request failed")
    }
}

impl std::error::Error for Wrapped {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.0)
    }
}

#[test]
fn only_a_top_level_broken_pipe_counts_as_a_closed_pipe() {
    let broken = anyhow::Error::from(Error::from(ErrorKind::BrokenPipe));
    assert!(gdcli::is_broken_pipe(&broken));
    assert!(gdcli::is_broken_pipe(
        &broken.context("cannot write output")
    ));
    let network = anyhow::Error::new(Wrapped(Error::from(ErrorKind::BrokenPipe)));
    assert!(!gdcli::is_broken_pipe(&network));
    assert!(!gdcli::is_broken_pipe(&network.context("request failed")));
    assert!(!gdcli::is_broken_pipe(&anyhow::Error::from(Error::from(
        ErrorKind::NotFound
    ))));
    assert!(!gdcli::is_broken_pipe(&anyhow::anyhow!("broken pipe")));
}

#[test]
fn interactive_commands_have_no_deadline_and_api_commands_have_one() {
    use clap::Parser;
    let parse = |args: &[&str]| {
        gdcli::Cli::try_parse_from(std::iter::once("gdcli").chain(args.iter().copied()))
            .unwrap()
            .command
    };
    for args in [&["setup"][..], &["login"]] {
        assert_eq!(deadline_for(&parse(args)), None, "{args:?}");
    }
    for args in [
        &["calendar", "list"][..],
        &["calendar", "get", "e"],
        &["meet", "list"],
        &["meet", "get", "e"],
        &["docs", "get", "d"],
    ] {
        assert_eq!(
            deadline_for(&parse(args)),
            Some(gdcli::google::DEADLINE),
            "{args:?}"
        );
    }
    assert!(matches!(parse(&["login"]), Command::Login));
}

#[test]
fn the_product_description_is_the_first_line_of_the_readme() {
    let readme =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/README.md")).unwrap();
    assert_eq!(readme.lines().next(), Some(env!("CARGO_PKG_DESCRIPTION")));
}

#[test]
fn help_shows_the_cargo_description() {
    use clap::CommandFactory;
    let about = gdcli::Cli::command().get_about().unwrap().to_string();
    assert_eq!(about, env!("CARGO_PKG_DESCRIPTION"));
}
