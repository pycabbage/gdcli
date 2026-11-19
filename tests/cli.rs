use clap::{CommandFactory, Parser};
use gdcli::Cli;
use gdcli::cli::{CalendarCommand, Command, DocsCommand, MeetCommand};

fn parse(args: &[&str]) -> Result<Command, clap::Error> {
    Cli::try_parse_from(std::iter::once("gdcli").chain(args.iter().copied())).map(|cli| cli.command)
}

#[test]
fn definition_is_consistent() {
    Cli::command().debug_assert();
}

#[test]
fn parses_every_specified_command() {
    assert!(matches!(parse(&["setup"]), Ok(Command::Setup)));
    assert!(matches!(parse(&["login"]), Ok(Command::Login)));
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
        Ok(Command::Docs(DocsCommand::Get { id, tab })) => {
            assert_eq!(id, "https://docs.google.com/document/d/X/edit");
            assert_eq!(tab.as_deref(), Some("Notes"));
        }
        other => panic!("{other:?}"),
    }
    match parse(&["docs", "metadata", "X", "--json"]) {
        Ok(Command::Docs(DocsCommand::Metadata { id, json })) => {
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
        &["setup", "--json"],
        &["login", "--json"],
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
        &["calendar", "list", "--limit", "3"],
        &["setup", "--force"],
        &["login", "--no-browser"],
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
