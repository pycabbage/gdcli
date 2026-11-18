use chrono::NaiveDate;
use gdcli::Paths;
use gdcli::calendar::events::{has_ended, instant, is_cancelled, render_time};
use gdcli::docs::document_id;
use gdcli::meet::{Target, is_meeting_code, meeting_code, parse_target, record_id};
use gdcli::output::{field, field_opt, row};
use google_calendar3::api::{Event, EventDateTime};
use serde_json::json;

fn event(value: serde_json::Value) -> Event {
    serde_json::from_value(value).unwrap()
}

fn at(value: &str) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::parse_from_rfc3339(value)
        .unwrap()
        .to_utc()
}

#[test]
fn meeting_codes_are_three_four_three_lowercase_letters() {
    for valid in ["abc-defg-hij", "xyz-wxyz-abc"] {
        assert!(is_meeting_code(valid), "{valid}");
    }
    for invalid in [
        "",
        "abc-defg-hi",
        "abc-defg-hijk",
        "ab-defg-hij",
        "ABC-defg-hij",
        "abc-def1-hij",
        "abc-defg-hij-klm",
        "abcdefghij",
        "abc_defg_hij",
        "wGVfAbCdE12",
    ] {
        assert!(!is_meeting_code(invalid), "{invalid}");
    }
}

#[test]
fn meet_ids_resolve_to_codes_or_record_names() {
    assert_eq!(
        parse_target("abc-defg-hij"),
        Target::Code("abc-defg-hij".to_owned())
    );
    assert_eq!(
        parse_target("https://meet.google.com/abc-defg-hij"),
        Target::Code("abc-defg-hij".to_owned())
    );
    assert_eq!(
        parse_target("ABC-Defg-HIJ"),
        Target::Code("abc-defg-hij".to_owned())
    );
    assert_eq!(
        parse_target("REC1"),
        Target::Record("conferenceRecords/REC1".to_owned())
    );
    assert_eq!(
        parse_target("conferenceRecords/REC1"),
        Target::Record("conferenceRecords/REC1".to_owned())
    );
    assert_eq!(record_id("conferenceRecords/REC1"), "REC1");
    assert_eq!(record_id("REC1"), "REC1");
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
    let mut out = Vec::new();
    row(&mut out, &["a", "b\tc", "d\ne\r", ""]).unwrap();
    assert_eq!(String::from_utf8(out).unwrap(), "a\tb c\td e \t\n");
}

#[test]
fn fields_skip_empty_values_and_indent_continuations() {
    let mut out = Vec::new();
    field(&mut out, "empty", "  \n ").unwrap();
    field_opt(&mut out, "none", None).unwrap();
    field_opt(&mut out, "some", Some("x")).unwrap();
    field(&mut out, "multi", "one\ntwo\n\nthree\n").unwrap();
    assert_eq!(
        String::from_utf8(out).unwrap(),
        "some: x\nmulti: one\n  two\n  \n  three\n"
    );
}

#[test]
fn event_times_render_as_rfc3339_or_dates() {
    let timed = EventDateTime {
        date_time: Some(at("2026-11-18T09:30:15+09:00")),
        ..Default::default()
    };
    let all_day = EventDateTime {
        date: NaiveDate::from_ymd_opt(2026, 11, 20),
        ..Default::default()
    };
    assert_eq!(render_time(Some(&timed)), "2026-11-18T00:30:15Z");
    assert_eq!(render_time(Some(&all_day)), "2026-11-20");
    assert_eq!(render_time(Some(&EventDateTime::default())), "");
    assert_eq!(render_time(None), "");
    assert_eq!(instant(&all_day), Some(at("2026-11-20T00:00:00Z")));
    assert_eq!(instant(&EventDateTime::default()), None);
}

#[test]
fn ended_events_have_an_end_at_or_before_now() {
    let now = at("2026-11-18T00:00:00Z");
    let ended = event(json!({"end": {"dateTime": "2026-11-18T00:00:00Z"}}));
    let ongoing = event(json!({"end": {"dateTime": "2026-11-18T00:00:01Z"}}));
    let all_day = event(json!({"end": {"date": "2026-11-18"}}));
    assert!(has_ended(&ended, now));
    assert!(!has_ended(&ongoing, now));
    assert!(has_ended(&all_day, now));
    assert!(!has_ended(&event(json!({})), now));
}

#[test]
fn cancelled_status_is_detected() {
    assert!(is_cancelled(&event(json!({"status": "cancelled"}))));
    assert!(!is_cancelled(&event(json!({"status": "confirmed"}))));
    assert!(!is_cancelled(&event(json!({}))));
}

#[test]
fn only_hangouts_meet_conferences_yield_a_code() {
    let meet = event(
        json!({"conferenceData": {"conferenceId": "abc-defg-hij", "conferenceSolution": {"key": {"type": "hangoutsMeet"}}}}),
    );
    let other = event(
        json!({"conferenceData": {"conferenceId": "123", "conferenceSolution": {"key": {"type": "addOn"}}}}),
    );
    let no_id =
        event(json!({"conferenceData": {"conferenceSolution": {"key": {"type": "hangoutsMeet"}}}}));
    assert_eq!(meeting_code(&meet), Some("abc-defg-hij"));
    assert_eq!(meeting_code(&other), None);
    assert_eq!(meeting_code(&no_id), None);
    assert_eq!(meeting_code(&event(json!({}))), None);
}

#[test]
fn paths_join_the_fixed_file_names() {
    let paths = Paths::new("/c/gdcli", "/d/gdcli");
    assert_eq!(
        paths.client_secret(),
        std::path::Path::new("/c/gdcli/client_secret.json")
    );
    assert_eq!(paths.token(), std::path::Path::new("/d/gdcli/token.json"));
    assert_eq!(paths.config_dir(), std::path::Path::new("/c/gdcli"));
    assert_eq!(paths.data_dir(), std::path::Path::new("/d/gdcli"));
}
