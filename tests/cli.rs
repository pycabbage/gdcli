use clap::{CommandFactory, Parser};
use gdcli::Cli;
use gdcli::cli::{AuthCommand, CalendarCommand, Command, DocsCommand, MeetCommand};

fn parse(args: &[&str]) -> Result<Command, clap::Error> {
    Cli::try_parse_from(std::iter::once("gdcli").chain(args.iter().copied())).map(|cli| cli.command)
}

#[test]
fn definition_is_consistent() {
    Cli::command().debug_assert();
}

#[test]
fn parses_every_specified_command() {
    assert!(matches!(
        parse(&["auth", "setup"]),
        Ok(Command::Auth(AuthCommand::Setup))
    ));
    assert!(matches!(
        parse(&["auth", "login"]),
        Ok(Command::Auth(AuthCommand::Login))
    ));
    assert!(matches!(
        parse(&["auth", "list"]),
        Ok(Command::Auth(AuthCommand::List))
    ));
    match parse(&["auth", "set-default", "Bob@Example.org"]) {
        Ok(Command::Auth(AuthCommand::SetDefault { email })) => {
            assert_eq!(email.to_string(), "bob@example.org");
        }
        other => panic!("{other:?}"),
    }
    assert!(matches!(parse(&["update"]), Ok(Command::Update)));
    match parse(&["meet", "list"]) {
        Ok(Command::Meet(MeetCommand::List(args))) => {
            assert_eq!((args.past, args.upcoming, args.json), (5, 5, false));
        }
        other => panic!("{other:?}"),
    }
    match parse(&["calendar", "list"]) {
        Ok(Command::Calendar(CalendarCommand::List(args))) => {
            assert_eq!((args.past, args.upcoming, args.json), (5, 5, false));
        }
        other => panic!("{other:?}"),
    }
    match parse(&["meet", "get", "evt1", "--json"]) {
        Ok(Command::Meet(MeetCommand::Get(args))) => {
            assert_eq!((args.id.as_str(), args.json), ("evt1", true));
        }
        other => panic!("{other:?}"),
    }
    match parse(&["calendar", "get", "evt1"]) {
        Ok(Command::Calendar(CalendarCommand::Get(args))) => {
            assert_eq!((args.id.as_str(), args.json), ("evt1", false));
        }
        other => panic!("{other:?}"),
    }
    match parse(&[
        "docs",
        "get",
        "https://docs.google.com/document/d/X/edit",
        "--tab",
        "Notes",
    ]) {
        Ok(Command::Docs(DocsCommand::Get { id, tab, account })) => {
            assert_eq!(id, "https://docs.google.com/document/d/X/edit");
            assert_eq!(tab.as_deref(), Some("Notes"));
            assert!(account.account.is_none());
        }
        other => panic!("{other:?}"),
    }
    match parse(&["docs", "metadata", "X", "--json"]) {
        Ok(Command::Docs(DocsCommand::Metadata { id, json, .. })) => {
            assert_eq!((id.as_str(), json), ("X", true));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn list_counts_default_to_five_and_accept_the_future_alias() {
    for group in ["calendar", "meet"] {
        let counts = |args: &[&str]| match parse(args) {
            Ok(
                Command::Calendar(CalendarCommand::List(list))
                | Command::Meet(MeetCommand::List(list)),
            ) => (list.past, list.upcoming),
            other => panic!("{other:?}"),
        };
        assert_eq!(
            counts(&[group, "list", "--past", "3", "--upcoming", "8"]),
            (3, 8)
        );
        assert_eq!(
            counts(&[group, "list", "--future", "0", "--past", "1000"]),
            (1000, 0)
        );
        for bad in ["1001", "-1", "x", ""] {
            assert!(parse(&[group, "list", "--past", bad]).is_err(), "{bad}");
            assert!(parse(&[group, "list", "--upcoming", bad]).is_err(), "{bad}");
        }
    }
}

#[test]
fn json_exists_exactly_where_the_owner_asked_for_it() {
    for args in [
        &["calendar", "list", "--json"][..],
        &["calendar", "get", "e", "--json"],
        &["meet", "list", "--json"],
        &["meet", "get", "e", "--json"],
        &["docs", "metadata", "d", "--json"],
    ] {
        assert!(parse(args).is_ok(), "{args:?}");
    }
    for args in [
        &["docs", "get", "d", "--json"][..],
        &["meet", "notes", "e", "--json"],
        &["meet", "transcript", "e", "--json"],
        &["auth", "setup", "--json"],
        &["auth", "login", "--json"],
        &["auth", "list", "--json"],
        &["auth", "set-default", "a@b.c", "--json"],
        &["update", "--json"],
        &["update", "now"],
        &["docs", "metadata", "d", "--tab", "x"],
        &["calendar", "get", "e", "--past", "1"],
    ] {
        assert!(parse(args).is_err(), "{args:?}");
    }
}

#[test]
fn rejects_unspecified_flags_and_missing_arguments() {
    for args in [
        &["docs", "get", "X", "--json"][..],
        &["docs", "get", "--json", "X"],
        &["calendar", "list", "--offset", "3"],
        &["setup"],
        &["login"],
        &["set-default", "a@b.c"],
        &["auth", "setup", "--force"],
        &["auth", "login", "--no-browser"],
        &["auth", "list", "x"],
        &["auth", "set-default"],
        &["auth", "set-default", "a@b.c", "d@e.f"],
        &["auth"],
        &["docs", "get"],
        &["meet", "get"],
        &["calendar", "get"],
        &["docs"],
        &["unknown"],
        &[],
    ] {
        assert!(parse(args).is_err(), "{args:?}");
    }
}

#[test]
fn rejects_empty_ids_with_a_usage_error() {
    for args in [
        &["calendar", "get", ""][..],
        &["meet", "get", ""],
        &["docs", "get", ""],
    ] {
        let error = parse(args).unwrap_err();
        assert_eq!(error.exit_code(), 2, "{args:?}");
    }
}

#[test]
fn rejects_document_ids_that_cannot_be_ids() {
    for id in [".", "..", "https://docs.google.com/document/d/../edit", ""] {
        for args in [["docs", "get", id], ["docs", "metadata", id]] {
            let error = parse(&args).unwrap_err();
            assert_eq!(error.exit_code(), 2, "{args:?}");
        }
    }
    assert!(parse(&["docs", "get", "https://docs.google.com/document/d/ABC/edit"]).is_ok());
}

#[test]
fn rejects_blank_tab_names() {
    for tab in ["", " ", "\t", "  \n "] {
        let error = parse(&["docs", "get", "d", "--tab", tab]).unwrap_err();
        assert_eq!(error.exit_code(), 2, "{tab:?}");
        assert!(error.to_string().contains("invalid tab"), "{error}");
    }
    assert!(parse(&["docs", "get", "d", "--tab", "t.abc"]).is_ok());
    assert!(parse(&["docs", "get", "d", "--tab", "My tab"]).is_ok());
}

#[test]
fn rejects_event_ids_that_could_change_the_request_path() {
    for id in [".", "..", "%2e", "%2e%2e", "a/b", "a?b", "a#b", "a%2Fb"] {
        for args in [["calendar", "get", id], ["meet", "get", id]] {
            let error = parse(&args).unwrap_err();
            assert_eq!(
                error.kind(),
                clap::error::ErrorKind::ValueValidation,
                "{args:?}"
            );
            assert!(error.to_string().contains("invalid event id"), "{error}");
        }
    }
    assert!(parse(&["calendar", "get", "abc123_20261120T010000Z"]).is_ok());
}

fn leaves(command: &clap::Command, path: &str, found: &mut Vec<(String, bool)>) {
    for sub in command
        .get_subcommands()
        .filter(|sub| sub.get_name() != "help")
    {
        let path = format!("{path} {}", sub.get_name()).trim().to_owned();
        if sub.has_subcommands() {
            leaves(sub, &path, found);
        } else {
            let has_account = sub
                .get_arguments()
                .any(|arg| arg.get_long() == Some("account") && arg.get_short() == Some('a'));
            found.push((path, has_account));
        }
    }
}

#[test]
fn account_option_exists_exactly_on_the_google_commands() {
    let mut found = Vec::new();
    leaves(&Cli::command(), "", &mut found);
    found.sort();
    let with = [
        "calendar get",
        "calendar list",
        "docs get",
        "docs metadata",
        "meet get",
        "meet list",
        "meet notes",
        "meet transcript",
    ];
    let without = [
        "auth list",
        "auth login",
        "auth set-default",
        "auth setup",
        "update",
    ];
    let mut expected: Vec<(String, bool)> = with
        .iter()
        .map(|path| ((*path).to_owned(), true))
        .chain(without.iter().map(|path| ((*path).to_owned(), false)))
        .collect();
    expected.sort();
    assert_eq!(found, expected);
}

#[test]
fn account_values_are_validated() {
    for args in [
        &["calendar", "list", "-a", "bad"][..],
        &["meet", "get", "e", "--account", "a/b@c.d"],
        &["docs", "get", "d", "-a", ""],
        &["docs", "metadata", "d", "--account", "../x@y.z"],
        &["meet", "notes", "e", "-a", "bad"],
        &["meet", "transcript", "e", "--account", "a b@c.d"],
        &["auth", "set-default", "../x@y.z"],
        &["auth", "set-default", "bad"],
    ] {
        let error = parse(args).unwrap_err();
        assert_eq!(error.exit_code(), 2, "{args:?}");
        assert!(
            error
                .to_string()
                .contains("expected an email address such as name@example.com"),
            "{error}"
        );
    }
    match parse(&["calendar", "list", "-a", " Alice@Example.COM "]) {
        Ok(Command::Calendar(CalendarCommand::List(args))) => {
            assert_eq!(
                args.account.account.unwrap().to_string(),
                "alice@example.com"
            );
        }
        other => panic!("{other:?}"),
    }
}

fn list_args(group: &str, extra: &[&str]) -> gdcli::cli::ListArgs {
    let mut args = vec![group, "list"];
    args.extend_from_slice(extra);
    match parse(&args) {
        Ok(
            Command::Calendar(CalendarCommand::List(list)) | Command::Meet(MeetCommand::List(list)),
        ) => list,
        other => panic!("{other:?}"),
    }
}

#[test]
fn from_conflicts_with_past_and_upcoming_and_query_composes_with_both() {
    for group in ["calendar", "meet"] {
        for extra in [
            &["--from", "2026-11-01", "--past", "3"][..],
            &["--from", "2026-11-01", "--upcoming", "3"],
            &["--from", "2026-11-01", "--future", "0"],
            &["--past", "3", "--from", "2026-11-01"],
        ] {
            let error = parse(&[&[group, "list"], extra].concat()).unwrap_err();
            assert_eq!(error.kind(), clap::error::ErrorKind::ArgumentConflict);
            assert_eq!(error.exit_code(), 2);
        }
        let list = list_args(
            group,
            &["--query", "standup", "--past", "10", "--upcoming", "0"],
        );
        assert_eq!(
            (list.query.as_deref(), list.past, list.upcoming, list.from),
            (Some("standup"), 10, 0, None)
        );
        let list = list_args(
            group,
            &[
                "--from",
                "2026-11-01",
                "--to",
                "2026-11-20T09:00:00+09:00",
                "--limit",
                "5",
                "--query",
                "a b",
                "--json",
            ],
        );
        assert_eq!(list.from.unwrap().unix_timestamp(), 1_793_491_200);
        assert_eq!(list.to.unwrap().unix_timestamp(), 1_795_132_800);
        assert_eq!(
            (list.limit, list.query.as_deref(), list.json),
            (Some(5), Some("a b"), true)
        );
        let list = list_args(group, &["--from", "2026-11-01"]);
        assert_eq!((list.to, list.limit, list.query), (None, None, None));
        let list = list_args(group, &[]);
        assert_eq!(
            (list.from, list.to, list.limit, list.past, list.upcoming),
            (None, None, None, 5, 5)
        );
    }
}

#[test]
fn to_and_limit_require_from() {
    for group in ["calendar", "meet"] {
        for extra in [
            &["--to", "2026-11-01"][..],
            &["--limit", "3"],
            &["--to", "2026-11-01", "--limit", "3", "--query", "x"],
        ] {
            let error = parse(&[&[group, "list"], extra].concat()).unwrap_err();
            assert_eq!(
                error.kind(),
                clap::error::ErrorKind::MissingRequiredArgument
            );
            assert_eq!(error.exit_code(), 2);
            assert!(error.to_string().contains("--from <TIME>"), "{error}");
        }
        for extra in [
            &["--past", "1", "--to", "2026-11-01"][..],
            &["--upcoming", "0", "--limit", "3"],
            &["--future", "2", "--to", "2026-11-01"],
            &["--limit", "3", "--past", "0"],
        ] {
            let error = parse(&[&[group, "list"], extra].concat()).unwrap_err();
            assert_eq!(
                error.kind(),
                clap::error::ErrorKind::ArgumentConflict,
                "{extra:?}"
            );
            assert_eq!(error.exit_code(), 2);
        }
    }
}

#[test]
fn limit_is_1_to_1000_and_query_must_not_be_blank() {
    for group in ["calendar", "meet"] {
        for bad in ["0", "1001", "-1", "x", "", "99999"] {
            let error =
                parse(&[group, "list", "--from", "2026-11-01", "--limit", bad]).unwrap_err();
            assert_eq!(error.exit_code(), 2, "{bad}");
        }
        let error = parse(&[group, "list", "--from", "2026-11-01", "--limit", "0"]).unwrap_err();
        assert!(error.to_string().contains("not in 1..=1000"), "{error}");
        for good in ["1", "50", "1000"] {
            let list = list_args(group, &["--from", "2026-11-01", "--limit", good]);
            assert_eq!(list.limit, good.parse().ok());
        }
        for blank in ["", " ", "\t", " \n "] {
            let error = parse(&[group, "list", "--query", blank]).unwrap_err();
            assert_eq!(error.exit_code(), 2, "{blank:?}");
            assert!(
                error
                    .to_string()
                    .contains("invalid query: it must not be blank"),
                "{error}"
            );
        }
        assert_eq!(
            list_args(group, &["--query", " padded "]).query.as_deref(),
            Some(" padded ")
        );
    }
}

#[test]
fn times_must_be_dates_or_rfc3339() {
    for group in ["calendar", "meet"] {
        for bound in ["--from", "--to"] {
            for bad in [
                "2026-11-31",
                "2026-1-5",
                "2026-02-29",
                "20261120",
                "yesterday",
                "",
                "2026-11-20 10:00",
                "2026-11-20T10:00:00",
                "2026-11-20T10:00Z",
            ] {
                let mut args = vec![group, "list", "--from", "2026-01-01"];
                args.extend([bound, bad]);
                if bound == "--from" {
                    args.drain(2..4);
                }
                let error = parse(&args).unwrap_err();
                assert_eq!(error.exit_code(), 2, "{bound} {bad}");
                let message = error.to_string();
                assert!(
                    message.contains(&format!(
                        "invalid value '{bad}' for '{bound} <TIME>': use YYYY-MM-DD or RFC 3339"
                    )),
                    "{message}"
                );
            }
        }
        for good in [
            "2026-11-20",
            "2026-11-20T10:00:00Z",
            "2026-11-20T10:00:00+09:00",
            "2026-11-20T10:00:00.250-05:00",
        ] {
            let list = list_args(group, &["--from", "2025-01-01", "--to", good]);
            assert!(list.to.is_some(), "{good}");
        }
    }
}

#[test]
fn notes_and_transcript_take_tab_account_and_no_json() {
    for (name, notes) in [("notes", true), ("transcript", false)] {
        let (id, tab, account) = match parse(&[
            "meet",
            name,
            "evt1",
            "--tab",
            "Full notes",
            "-a",
            "Bob@Example.org",
        ]) {
            Ok(Command::Meet(MeetCommand::Notes(args))) if notes => {
                (args.id, args.tab, args.account)
            }
            Ok(Command::Meet(MeetCommand::Transcript(args))) if !notes => {
                (args.id, args.tab, args.account)
            }
            other => panic!("{other:?}"),
        };
        assert_eq!((id.as_str(), tab.as_deref()), ("evt1", Some("Full notes")));
        assert_eq!(account.account.unwrap().to_string(), "bob@example.org");
        match parse(&["meet", name, "evt1"]) {
            Ok(Command::Meet(MeetCommand::Notes(args) | MeetCommand::Transcript(args))) => {
                assert!(args.tab.is_none() && args.account.account.is_none());
            }
            other => panic!("{other:?}"),
        }
        for args in [
            &["meet", name][..],
            &["meet", name, "e", "--json"],
            &["meet", name, "e", "--from", "2026-11-01"],
            &["meet", name, "e", "--limit", "3"],
            &["meet", name, "a", "b"],
        ] {
            assert!(parse(args).is_err(), "{args:?}");
        }
        for blank in ["", " ", "\t"] {
            let error = parse(&["meet", name, "e", "--tab", blank]).unwrap_err();
            assert_eq!(error.exit_code(), 2);
            assert!(error.to_string().contains("invalid tab"), "{error}");
        }
        for id in ["", ".", "..", "a/b", "a?b", "a#b", "a%2Fb"] {
            let error = parse(&["meet", name, id]).unwrap_err();
            assert_eq!(error.exit_code(), 2, "{id}");
        }
    }
}

#[test]
fn help_and_version_are_available() {
    for args in [&["--help"][..], &["--version"], &["docs", "get", "--help"]] {
        let error = parse(args).unwrap_err();
        assert_eq!(
            error.kind(),
            if args[0] == "--version" {
                clap::error::ErrorKind::DisplayVersion
            } else {
                clap::error::ErrorKind::DisplayHelp
            }
        );
    }
}
