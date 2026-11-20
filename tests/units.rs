mod common;

use gdcli::cli::moment;
use gdcli::docs::document_id;
use gdcli::events::{meeting_code, render_time, stamp, stamp_millis};
use gdcli::output::{compact, record, table};
use gdcli::update::{asset, bearer, github, github_token, version};
use gdcli::{AuthCommand, Command, Paths, deadline_for};
use serde_json::{Value, json};
use std::ffi::OsString;
use std::io::{Error, ErrorKind};
use std::os::unix::ffi::OsStringExt;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use url::Url;

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

fn sample() -> serde_json::Value {
    json!({
        "id": "abc",
        "summary": "Planning",
        "start": "2026-11-20T01:00:00Z",
        "organizer": {"name": "Boss", "email": "boss@example.com"},
        "attendees": [
            {"name": "Alice", "email": "alice@example.com", "response_status": "accepted", "optional": true},
            {"email": "bee@example.com", "response_status": "needsAction", "optional": false}
        ],
        "recurrence": ["RRULE:FREQ=WEEKLY", "EXDATE:20261127"],
        "shared": false,
        "version": 12,
        "conferences": [
            {"id": "REC1", "start": "s1", "participants": [{"name": "Guest", "kind": "anonymous"}], "recordings": []},
            {"id": "REC2"}
        ],
        "description": "First\r\nSecond\rThird",
    })
}

#[test]
fn records_render_name_value_pairs_for_objects_and_one_line_per_array_element() {
    let text = written(|out| record(out, &sample(), false).unwrap());
    assert_eq!(
        text,
        "id: abc\nsummary: Planning\nstart: 2026-11-20T01:00:00Z\n\
         organizer: name=Boss email=boss@example.com\n\
         attendee: name=Alice email=alice@example.com response_status=accepted optional=true\n\
         attendee: email=bee@example.com response_status=needsAction optional=false\n\
         recurrence: RRULE:FREQ=WEEKLY\nrecurrence: EXDATE:20261127\n\
         shared: false\nversion: 12\n\
         conference: id=REC1 start=s1\nparticipant: name=Guest kind=anonymous\nrecording: none\n\
         conference: id=REC2\n\
         description: First\n  Second\n  Third\n"
    );
}

#[test]
fn pair_values_are_bare_unless_they_could_be_misread() {
    let value = json!({"items": [{"a": "x y", "b": "", "c": "q\"r", "d": "e=f", "e": "plain", "f": false, "g": 0, "h": "tab\there", "i": "é"}]});
    let text = written(|out| record(out, &value, false).unwrap());
    assert_eq!(
        text,
        "item: a=\"x y\" b=\"\" c=\"q\\\"r\" d=\"e=f\" e=plain f=false g=0 h=\"tab\\there\" i=é\n"
    );
}

#[test]
fn an_unexpected_nested_object_is_rendered_as_a_quoted_json_string_never_dropped() {
    let value = json!({"things": [{"a": 1, "inner": {"b": "x y", "c": [1, 2]}, "z": true}], "box": {"deep": {"k": "v"}}});
    let text = written(|out| record(out, &value, false).unwrap());
    assert_eq!(
        text,
        "thing: a=1 inner=\"{\\\"b\\\":\\\"x y\\\",\\\"c\\\":[1,2]}\" z=true\n\
         box: deep=\"{\\\"k\\\":\\\"v\\\"}\"\n"
    );
    common::assert_same_information(&text, &value.to_string());
}

#[test]
fn top_level_values_are_trimmed_of_surrounding_whitespace() {
    let text =
        written(|out| record(out, &json!({"a": "  padded \t", "b": "\n x \n"}), false).unwrap());
    assert_eq!(text, "a: padded\nb: x\n");
}

#[test]
fn the_json_rendering_is_the_record_itself_on_one_line() {
    let json = written(|out| record(out, &sample(), true).unwrap());
    assert_eq!(json, format!("{}\n", sample()));
    assert!(json.starts_with(r#"{"id":"abc","summary":"Planning","start""#));
}

#[test]
fn text_and_json_carry_the_same_information_line_by_line() {
    let text = written(|out| record(out, &sample(), false).unwrap());
    common::assert_same_information(&text, &sample().to_string());
}

#[test]
#[should_panic(expected = "assertion")]
fn the_equivalence_check_notices_a_nested_false_dropped_from_the_text() {
    let text = written(|out| record(out, &sample(), false).unwrap()).replace(" optional=false", "");
    common::assert_same_information(&text, &sample().to_string());
}

#[test]
#[should_panic(expected = "assertion")]
fn the_equivalence_check_notices_a_field_dropped_from_the_text() {
    let text = written(|out| record(out, &sample(), false).unwrap()).replace(" kind=anonymous", "");
    common::assert_same_information(&text, &sample().to_string());
}

#[test]
#[should_panic(expected = "assertion")]
fn the_equivalence_check_notices_values_moved_to_another_line() {
    let text = written(|out| record(out, &sample(), false).unwrap())
        .replace("summary: Planning\nstart", "start");
    common::assert_same_information(&text, &sample().to_string());
}

#[test]
fn compact_drops_nulls_and_empties_but_keeps_false_and_zero() {
    let value = json!({"a": null, "b": "", "c": " ", "d": [], "e": {}, "f": [null, "x", ""], "g": false, "h": 0, "i": {"j": null}});
    assert_eq!(
        compact(value),
        Some(json!({"f": ["x"], "g": false, "h": 0}))
    );
    assert_eq!(compact(json!({"a": null})), None);
}

#[test]
fn tables_print_a_header_and_rows_or_an_array_of_objects() {
    let rows = vec![
        vec!["1".to_owned(), "a\tb\nc\r".to_owned()],
        vec!["2".to_owned(), String::new()],
    ];
    assert_eq!(
        written(|out| table(out, "id\tname", &rows, false).unwrap()),
        "id\tname\n1\ta b c \n2\t\n"
    );
    assert_eq!(
        written(|out| table(out, "id\tname", &rows, true).unwrap()),
        "[{\"id\":\"1\",\"name\":\"a\\tb\\nc\\r\"},{\"id\":\"2\",\"name\":\"\"}]\n"
    );
    assert_eq!(
        written(|out| table(out, "id\tname", &[], false).unwrap()),
        "id\tname\n"
    );
    assert_eq!(
        written(|out| table(out, "id\tname", &[], true).unwrap()),
        "[]\n"
    );
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
    assert_eq!(
        paths.legacy_token(),
        std::path::Path::new("/d/gdcli/token.json")
    );
    assert_eq!(paths.accounts(), std::path::Path::new("/d/gdcli/accounts"));
    assert_eq!(
        paths.default_account(),
        std::path::Path::new("/d/gdcli/default")
    );
    assert_eq!(
        paths.account(&"A@B.c".parse().unwrap()),
        std::path::Path::new("/d/gdcli/accounts/a@b.c.json")
    );
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
    for args in [&["auth", "setup"][..], &["auth", "login"]] {
        assert_eq!(deadline_for(&parse(args)), None, "{args:?}");
    }
    for args in [
        &["calendar", "list"][..],
        &["calendar", "get", "e"],
        &["meet", "list"],
        &["meet", "get", "e"],
        &["docs", "get", "d"],
        &["auth", "list"],
        &["auth", "set-default", "a@b.c"],
        &["skills"],
        &["update"],
    ] {
        assert_eq!(
            deadline_for(&parse(args)),
            Some(gdcli::google::DEADLINE),
            "{args:?}"
        );
    }
    assert!(matches!(
        parse(&["auth", "login"]),
        Command::Auth(AuthCommand::Login)
    ));
}

#[tokio::test]
async fn google_commands_report_the_path_error() {
    use clap::Parser;
    for args in [
        &["calendar", "list"][..],
        &["calendar", "get", "e"],
        &["meet", "list"],
        &["meet", "get", "e"],
        &["meet", "notes", "e"],
        &["meet", "transcript", "e"],
        &["docs", "get", "X"],
        &["docs", "metadata", "X"],
        &["auth", "setup"],
        &["auth", "login"],
        &["auth", "list"],
        &["auth", "set-default", "a@b.c"],
    ] {
        let cli = gdcli::Cli::try_parse_from(std::iter::once("gdcli").chain(args.iter().copied()))
            .unwrap();
        let mut out = Vec::new();
        let error = gdcli::run_with(
            cli,
            &|| anyhow::bail!("cannot resolve XDG config home: x"),
            &mut out,
        )
        .await
        .unwrap_err();
        assert_eq!(out, b"", "{args:?}");
        assert_eq!(
            gdcli::message(&error),
            "cannot resolve XDG config home: x",
            "{args:?}"
        );
    }
}

#[test]
fn moment_parses_dates_and_rfc3339() {
    for (text, expected) in [
        ("2026-11-20", "2026-11-20T00:00:00Z"),
        ("2028-02-29", "2028-02-29T00:00:00Z"),
        ("2026-11-20T00:00:00Z", "2026-11-20T00:00:00Z"),
        ("2026-11-20T00:00:00+09:00", "2026-11-19T15:00:00Z"),
        ("2026-11-20T10:00:00-05:00", "2026-11-20T15:00:00Z"),
        ("2026-11-20T10:00:00.250Z", "2026-11-20T10:00:00.25Z"),
    ] {
        assert_eq!(moment(text).unwrap(), at(expected), "{text}");
    }
    assert_eq!(moment("2026-11-20").unwrap().offset().whole_seconds(), 0);
}

#[test]
fn moment_rejects_calendar_nonsense() {
    for text in [
        "2026-11-31",
        "2026-02-29",
        "2026-13-01",
        "2026-00-10",
        "2026-11-00",
        "2026-1-5",
        "20261120",
        "",
        "x",
        "2026-11-20 10:00:00",
        "2026-11-20T10:00:00",
        "2026-11-20T25:00:00Z",
        "2026-11-20T10:60:00Z",
        "2026/11/20",
        "2026-11-2a",
        " 2026-11-20",
        "\u{ff12}\u{ff10}\u{ff12}\u{ff16}-11-20",
    ] {
        assert_eq!(
            moment(text).unwrap_err(),
            "use YYYY-MM-DD or RFC 3339",
            "{text:?}"
        );
    }
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

#[test]
fn xdg_directories_follow_the_spec_and_fall_back_to_the_passwd_home() {
    let resolve = |vars: &[(&str, &str)], home: Option<&str>| {
        let env = |name: &str| {
            vars.iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| std::ffi::OsString::from(value))
        };
        Paths::resolve(&env, &|| home.map(std::path::PathBuf::from))
    };
    let paths = resolve(&[("XDG_CONFIG_HOME", "/c"), ("XDG_DATA_HOME", "/d")], None).unwrap();
    assert_eq!(
        (paths.config, paths.data),
        ("/c/gdcli".into(), "/d/gdcli".into())
    );
    let paths = resolve(&[("HOME", "/h")], None).unwrap();
    assert_eq!(
        (paths.config, paths.data),
        ("/h/.config/gdcli".into(), "/h/.local/share/gdcli".into())
    );
    let paths = resolve(
        &[
            ("XDG_CONFIG_HOME", "relative"),
            ("XDG_DATA_HOME", ""),
            ("HOME", "/h"),
        ],
        None,
    )
    .unwrap();
    assert_eq!(paths.config, std::path::Path::new("/h/.config/gdcli"));
    assert_eq!(paths.data, std::path::Path::new("/h/.local/share/gdcli"));
    let paths = resolve(&[], Some("/passwd")).unwrap();
    assert_eq!(paths.config, std::path::Path::new("/passwd/.config/gdcli"));
    let paths = resolve(&[("HOME", "relative")], Some("/passwd")).unwrap();
    assert_eq!(
        paths.data,
        std::path::Path::new("/passwd/.local/share/gdcli")
    );
    let error = resolve(&[], None).unwrap_err();
    assert_eq!(
        gdcli::message(&error),
        "cannot resolve XDG config home: set XDG_CONFIG_HOME or HOME"
    );
}

#[test]
fn github_hosts_are_allowed_only_over_https_without_userinfo_or_a_non_default_port() {
    for (url, allowed) in [
        ("https://github.com/x", true),
        ("https://api.github.com/x", true),
        ("https://release-assets.githubusercontent.com/x", true),
        ("https://github.com:443/x", true),
        ("https://github.com:8443/x", false),
        ("https://user@github.com/x", false),
        ("https://:pw@github.com/x", false),
        ("https://raw.github.com/x", false),
        ("http://github.com/x", false),
        ("https://githubusercontent.com/x", false),
        ("https://github.com.evil.com/x", false),
        ("https://evilgithub.com/x", false),
        ("https://github.com@evil.com/x", false),
        ("https://evil.com/github.com", false),
        ("https://github.com./x", false),
        ("ftp://github.com/x", false),
        ("https://127.0.0.1/x", false),
        ("https://[::1]/x", false),
    ] {
        assert_eq!(github(&Url::parse(url).unwrap()), allowed, "{url}");
    }
}

#[test]
fn the_release_tag_must_be_v_plus_a_stable_semver() {
    assert_eq!(
        version(&json!({"tag_name": "v0.2.0"})).unwrap().to_string(),
        "0.2.0"
    );
    let malformed =
        |tag: &str| format!("latest release tag {tag:?} is not v<major>.<minor>.<patch>");
    for (release, tag) in [
        (json!({"tag_name": "0.2.0"}), "0.2.0"),
        (json!({"tag_name": "v0.2"}), "v0.2"),
        (json!({"tag_name": "latest"}), "latest"),
        (json!({"tag_name": ""}), ""),
        (json!({"tag_name": 5}), ""),
        (json!({}), ""),
    ] {
        assert_eq!(
            version(&release).unwrap_err().to_string(),
            malformed(tag),
            "{release}"
        );
    }
    assert_eq!(
        version(&json!({"tag_name": "v1.0.0-rc.1"}))
            .unwrap_err()
            .to_string(),
        "latest release v1.0.0-rc.1 is a prerelease and is not installed"
    );
}

fn entry(name: &str) -> Value {
    json!({
        "name": name,
        "browser_download_url": format!("https://github.com/x/{name}"),
        "digest": format!("sha256:{name}-digest"),
    })
}

fn release(assets: &Value) -> Value {
    json!({"tag_name": "v0.2.0", "immutable": true, "assets": assets})
}

#[test]
fn the_asset_is_picked_by_its_exact_platform_name() {
    let release = release(&json!([
        entry("gdcli_checksums.txt"),
        entry("gdcli_linux_aarch64.tar.gz"),
        entry("gdcli_linux_x86_64.tar.gz"),
        entry("xgdcli_linux_riscv64.tar.gz"),
        entry("gdcli_linux_riscv64.tar.gz.sig"),
    ]));
    for platform in ["linux_x86_64", "linux_aarch64"] {
        let name = format!("gdcli_{platform}.tar.gz");
        assert_eq!(
            asset(&release, platform).unwrap(),
            (
                format!("https://github.com/x/{name}").as_str(),
                format!("{name}-digest").as_str()
            )
        );
    }
    for platform in ["macos_aarch64", "linux_riscv64", "linux_x86"] {
        assert_eq!(
            asset(&release, platform).unwrap_err().to_string(),
            format!("release v0.2.0 has no asset gdcli_{platform}.tar.gz")
        );
    }
}

#[test]
fn an_asset_without_a_download_url_a_sha256_digest_or_an_immutable_release_is_refused() {
    let name = "gdcli_linux_x86_64.tar.gz";
    let url = "https://github.com/x/gdcli";
    for broken in [
        json!({"name": name, "digest": "sha256:ab"}),
        json!({"name": name, "browser_download_url": url}),
        json!({"name": name, "browser_download_url": url, "digest": null}),
        json!({"name": name, "browser_download_url": url, "digest": "sha1:ab"}),
        json!({"name": name, "browser_download_url": url, "digest": "ab"}),
    ] {
        assert_eq!(
            asset(&release(&json!([broken])), "linux_x86_64")
                .unwrap_err()
                .to_string(),
            format!("asset {name} has no download url or sha256 digest")
        );
    }
    for immutable in [json!(false), json!(null), json!("true")] {
        let mut mutable = release(&json!([entry(name)]));
        mutable["immutable"] = immutable;
        assert_eq!(
            asset(&mutable, "linux_x86_64").unwrap_err().to_string(),
            "release v0.2.0 is not immutable and is not installed"
        );
    }
    let mut unreported = release(&json!([entry(name)]));
    unreported.as_object_mut().unwrap().remove("immutable");
    assert!(asset(&unreported, "linux_x86_64").is_err());
}

#[test]
fn the_github_token_variable_is_ignored_when_unset_or_empty_and_read_lossily_when_not_unicode() {
    assert_eq!(github_token(None), None);
    assert_eq!(github_token(Some(OsString::new())), None);
    assert_eq!(
        github_token(Some(OsString::from_vec(vec![b'g', 0xff]))),
        Some("g\u{fffd}".to_owned())
    );
    assert_eq!(
        github_token(Some(OsString::from("ghp_testtoken"))),
        Some("ghp_testtoken".to_owned())
    );
}

#[test]
fn the_bearer_header_is_sensitive_and_a_bad_token_is_refused_without_echoing_it() {
    let value = bearer("ghp_testtoken").unwrap();
    assert_eq!(value, "Bearer ghp_testtoken");
    assert!(value.is_sensitive());
    assert!(bearer("g\u{fffd}").is_ok());
    assert_eq!(format!("{value:?}"), "Sensitive");
    for bad in ["ghp_secret\nsecond", "ghp_secret\r", "ghp_secret\u{7f}"] {
        let error = format!("{:#}", bearer(bad).unwrap_err());
        assert_eq!(error, "GITHUB_TOKEN is not a valid HTTP header value");
        assert!(!error.contains("ghp_secret"), "{error}");
    }
}
