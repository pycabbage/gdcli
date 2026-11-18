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
    assert!(matches!(
        parse(&["meet", "list"]),
        Ok(Command::Meet(MeetCommand::List))
    ));
    assert!(matches!(
        parse(&["calendar", "list"]),
        Ok(Command::Calendar(CalendarCommand::List))
    ));
    match parse(&["meet", "get", "abc-defg-hij"]) {
        Ok(Command::Meet(MeetCommand::Get { id })) => assert_eq!(id, "abc-defg-hij"),
        other => panic!("{other:?}"),
    }
    match parse(&["calendar", "get", "evt1"]) {
        Ok(Command::Calendar(CalendarCommand::Get { id })) => assert_eq!(id, "evt1"),
        other => panic!("{other:?}"),
    }
    match parse(&["docs", "get", "https://docs.google.com/document/d/X/edit"]) {
        Ok(Command::Docs(DocsCommand::Get { id })) => {
            assert_eq!(id, "https://docs.google.com/document/d/X/edit")
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn rejects_unspecified_flags_and_missing_arguments() {
    for args in [
        &["docs", "get", "X", "--json"][..],
        &["docs", "get", "--json", "X"],
        &["calendar", "list", "--limit", "3"],
        &["meet", "list", "--json"],
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
