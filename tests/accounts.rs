mod common;

use common::Harness;
use gdcli::Paths;
use gdcli::auth::accounts::{
    Email, Stored, adopt, create, default_account, email, list, migrate, set_default, store,
    token_file,
};
use gdcli::auth::{SCOPES, authenticator, render};
use gdcli::google::{connector, https_builder};
use serde_json::json;
use std::io::Write;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use tempfile::{NamedTempFile, TempDir};

const EXPIRED_TOKEN: &str = r#"[{"scopes":["https://www.googleapis.com/auth/calendar.events.readonly","https://www.googleapis.com/auth/meetings.space.readonly","https://www.googleapis.com/auth/drive.readonly"],"token":{"access_token":"old","refresh_token":null,"expires_at":[2000,1,0,0,0,0,0,0,0],"id_token":null}}]"#;
const BAD_EMAIL: &str = "expected an email address such as name@example.com";
const NOBODY: &str = "no account is logged in: run `gdcli auth login`";

fn paths(dir: &TempDir) -> Paths {
    Paths::new(dir.path().join("cfg"), dir.path().join("data"))
}

fn mail(text: &str) -> Email {
    text.parse().unwrap()
}

fn mode(path: &Path) -> u32 {
    std::fs::metadata(path).unwrap().permissions().mode() & 0o777
}

struct Restore(PathBuf, u32);

impl Drop for Restore {
    fn drop(&mut self) {
        let _ = std::fs::set_permissions(&self.0, PermissionsExt::from_mode(self.1));
    }
}

fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    names
}

fn place(paths: &Paths, address: &str, content: &str) {
    std::fs::create_dir_all(paths.accounts()).unwrap();
    std::fs::write(paths.account(&mail(address)), content).unwrap();
}

fn point(paths: &Paths, text: &str) {
    std::fs::create_dir_all(&paths.data).unwrap();
    std::fs::write(paths.default_account(), text).unwrap();
}

fn legacy(paths: &Paths, content: &str) {
    std::fs::create_dir_all(&paths.data).unwrap();
    std::fs::write(paths.legacy_token(), content).unwrap();
    std::fs::set_permissions(paths.legacy_token(), PermissionsExt::from_mode(0o644)).unwrap();
}

fn pending(paths: &Paths, content: &str) -> NamedTempFile {
    std::fs::create_dir_all(paths.accounts()).unwrap();
    let mut file = NamedTempFile::with_prefix_in(".login-", paths.accounts()).unwrap();
    file.write_all(content.as_bytes()).unwrap();
    file
}

fn rendered(stored: &Stored) -> String {
    let mut out = Vec::new();
    render(stored, &mut out).unwrap();
    String::from_utf8(out).unwrap()
}

fn error_of<T>(result: anyhow::Result<T>) -> String {
    gdcli::message(&result.err().expect("an error"))
}

fn secret() -> yup_oauth2::ApplicationSecret {
    yup_oauth2::ApplicationSecret {
        client_id: "id.apps.googleusercontent.com".to_owned(),
        client_secret: "s".to_owned(),
        token_uri: "https://oauth2.googleapis.com/token".to_owned(),
        auth_uri: "https://accounts.google.com/o/oauth2/auth".to_owned(),
        redirect_uris: vec!["http://localhost".to_owned()],
        ..Default::default()
    }
}

#[test]
fn email_accepts_valid_addresses_and_lowercases() {
    let long = format!("{}@example.com", "a".repeat(116));
    assert_eq!(long.len(), 128);
    for (raw, expected) in [
        ("alice@example.com", "alice@example.com"),
        (
            "  Bob.Smith+Tag@Sub.Example.ORG \n",
            "bob.smith+tag@sub.example.org",
        ),
        ("a_b-c@x-y.z", "a_b-c@x-y.z"),
        ("1@2.3", "1@2.3"),
        (long.as_str(), long.as_str()),
    ] {
        assert_eq!(mail(raw).to_string(), expected, "{raw:?}");
    }
}

#[test]
fn email_rejects_traversal_and_malformed_values() {
    let long = format!("{}@example.com", "a".repeat(117));
    assert_eq!(long.len(), 129);
    for raw in [
        "../x@y.z",
        "a/b@c.d",
        "a\\b@c.d",
        "a\0b@c.d",
        "a b@c.d",
        "a@b c.d",
        "..",
        "../..",
        ".a@b.c",
        "a..b@c.d",
        "a@b..c",
        "abc",
        "a@",
        "@b.c",
        "a@b@c.d",
        "\u{e9}@example.com",
        "a@ex\u{e4}mple.com",
        "",
        "   ",
        long.as_str(),
    ] {
        assert_eq!(raw.parse::<Email>().unwrap_err(), BAD_EMAIL, "{raw:?}");
    }
}

#[test]
fn file_names_are_email_dot_json_below_accounts() {
    let paths = Paths::new("/c/gdcli", "/d/gdcli");
    for (raw, expected) in [
        ("alice@example.com", "alice@example.com.json"),
        ("A+B@Example.COM", "a+b@example.com.json"),
        ("x@y.z", "x@y.z.json"),
    ] {
        let file = paths.account(&mail(raw));
        assert_eq!(file.parent(), Some(paths.accounts().as_path()), "{raw}");
        assert_eq!(file.file_name().unwrap(), expected, "{raw}");
    }
    assert_eq!(paths.accounts(), Path::new("/d/gdcli/accounts"));
}

#[test]
fn list_without_a_directory_is_empty() {
    let dir = tempfile::tempdir().unwrap();
    let paths = paths(&dir);
    assert_eq!(list(&paths).unwrap(), Vec::<Email>::new());
    assert!(!paths.data.exists());
    std::fs::create_dir_all(paths.accounts()).unwrap();
    assert_eq!(list(&paths).unwrap(), Vec::<Email>::new());
}

#[test]
fn list_reports_an_unreadable_accounts_path_and_a_pointer_that_is_a_directory() {
    let dir = tempfile::tempdir().unwrap();
    let paths = paths(&dir);
    std::fs::create_dir_all(&paths.data).unwrap();
    std::fs::write(paths.accounts(), "").unwrap();
    let expected = format!(
        "cannot read {}: Not a directory",
        paths.accounts().display()
    );
    for message in [
        error_of(list(&paths)),
        error_of(default_account(&paths)),
        error_of(token_file(&paths, None)),
    ] {
        assert!(message.starts_with(&expected), "{message}");
        assert!(message.ends_with("(os error 20)"), "{message}");
    }
    std::fs::remove_file(paths.accounts()).unwrap();
    std::fs::create_dir(paths.default_account()).unwrap();
    let message = error_of(default_account(&paths));
    assert_eq!(
        message,
        format!(
            "cannot read {}: Is a directory (os error 21)",
            paths.default_account().display()
        )
    );
    std::fs::remove_dir(paths.default_account()).unwrap();
    place(&paths, "alice@example.com", "[]");
    point(&paths, "alice@example.com\n");
    let readable_dir = |path: &Path| {
        std::fs::set_permissions(path, PermissionsExt::from_mode(0o000)).unwrap();
        std::fs::read_dir(path).is_ok()
    };
    let readable_file = |path: &Path| {
        std::fs::set_permissions(path, PermissionsExt::from_mode(0o000)).unwrap();
        std::fs::read(path).is_ok()
    };
    let expected = |path: &Path| {
        format!(
            "cannot read {}: Permission denied (os error 13)",
            path.display()
        )
    };
    {
        let _restore = Restore(paths.accounts(), 0o700);
        if readable_dir(&paths.accounts()) {
            eprintln!("skipped: running as root (accounts)");
        } else {
            assert_eq!(error_of(list(&paths)), expected(&paths.accounts()));
        }
    }
    {
        let _restore = Restore(paths.default_account(), 0o600);
        if readable_file(&paths.default_account()) {
            eprintln!("skipped: running as root (pointer)");
        } else {
            assert_eq!(
                error_of(default_account(&paths)),
                expected(&paths.default_account())
            );
        }
    }
}

#[test]
fn list_ignores_temp_files_foreign_names_and_directories_and_sorts() {
    let dir = tempfile::tempdir().unwrap();
    let paths = paths(&dir);
    for address in ["zeta@example.com", "alpha@example.com", "mid@example.org"] {
        place(&paths, address, "[]");
    }
    let accounts = paths.accounts();
    for name in [
        ".login-AbC123",
        ".hidden@example.com.json",
        "notes.txt",
        "Upper@Example.com.json",
        " alpha@example.com.json",
        "x@y.z.json.bak",
        "bad..name@x.y.json",
        "@x.y.json",
        ".json",
    ] {
        std::fs::write(accounts.join(name), "[]").unwrap();
    }
    std::fs::create_dir(accounts.join("dir@example.com.json")).unwrap();
    let found: Vec<String> = list(&paths).unwrap().iter().map(Email::to_string).collect();
    assert_eq!(
        found,
        ["alpha@example.com", "mid@example.org", "zeta@example.com"]
    );
}

#[test]
fn default_uses_the_pointer() {
    let dir = tempfile::tempdir().unwrap();
    let paths = paths(&dir);
    place(&paths, "a@example.com", "[]");
    place(&paths, "b@example.com", "[]");
    point(&paths, "b@example.com\n");
    assert_eq!(default_account(&paths).unwrap(), mail("b@example.com"));
    point(&paths, "  A@Example.com ");
    assert_eq!(default_account(&paths).unwrap(), mail("a@example.com"));
}

#[test]
fn default_without_pointer_uses_the_only_account() {
    let dir = tempfile::tempdir().unwrap();
    let paths = paths(&dir);
    place(&paths, "only@example.com", "[]");
    assert_eq!(default_account(&paths).unwrap(), mail("only@example.com"));
    let (email, file) = token_file(&paths, None).unwrap();
    assert_eq!(email, mail("only@example.com"));
    assert_eq!(file, paths.account(&email));
}

#[test]
fn default_without_pointer_and_with_several_accounts_asks_for_set_default() {
    let dir = tempfile::tempdir().unwrap();
    let paths = paths(&dir);
    place(&paths, "b@example.com", "[]");
    place(&paths, "a@example.com", "[]");
    let expected = "no default account; run `gdcli auth set-default <email>`; known accounts: a@example.com, b@example.com";
    assert_eq!(error_of(default_account(&paths)), expected);
    assert_eq!(error_of(token_file(&paths, None)), expected);
}

#[test]
fn default_pointing_at_a_missing_token_names_the_fix() {
    let dir = tempfile::tempdir().unwrap();
    let paths = paths(&dir);
    place(&paths, "alice@example.com", "[]");
    point(&paths, "ghost@example.com\n");
    assert_eq!(default_account(&paths).unwrap(), mail("ghost@example.com"));
    assert_eq!(
        error_of(token_file(&paths, None)),
        "default account ghost@example.com has no token: run `gdcli auth login` or `gdcli auth set-default <email>`; known accounts: alice@example.com"
    );
}

#[test]
fn a_tampered_pointer_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let paths = paths(&dir);
    place(&paths, "alice@example.com", "[]");
    std::fs::write(paths.data.join("evil.json"), "[]").unwrap();
    for text in [
        "../evil\n",
        "../../etc/passwd",
        "a/b@c.d",
        "",
        "x@y.z\nw@y.z",
    ] {
        point(&paths, text);
        for result in [
            error_of(default_account(&paths)),
            error_of(token_file(&paths, None)),
        ] {
            assert!(
                result.starts_with(&format!(
                    "invalid default account in {}: {BAD_EMAIL}; run `gdcli auth set-default <email>`",
                    paths.default_account().display()
                )),
                "{text:?}: {result}"
            );
        }
    }
}

#[test]
fn requested_account_beats_the_default_and_is_case_insensitive() {
    let dir = tempfile::tempdir().unwrap();
    let paths = paths(&dir);
    place(&paths, "a@example.com", "[]");
    place(&paths, "b@example.com", "[]");
    point(&paths, "a@example.com\n");
    let (email, file) = token_file(&paths, Some(&mail("B@Example.COM"))).unwrap();
    assert_eq!(email, mail("b@example.com"));
    assert_eq!(file, paths.account(&email));
    assert_eq!(token_file(&paths, None).unwrap().0, mail("a@example.com"));
}

#[test]
fn unknown_account_lists_the_known_accounts() {
    let dir = tempfile::tempdir().unwrap();
    let paths = paths(&dir);
    place(&paths, "b@example.com", "[]");
    place(&paths, "a@example.com", "[]");
    point(&paths, "a@example.com\n");
    assert_eq!(
        error_of(token_file(&paths, Some(&mail("x@y.z")))),
        "unknown account x@y.z; known accounts: a@example.com, b@example.com"
    );
}

#[test]
fn no_account_says_to_log_in() {
    let dir = tempfile::tempdir().unwrap();
    let paths = paths(&dir);
    assert_eq!(error_of(default_account(&paths)), NOBODY);
    assert_eq!(error_of(token_file(&paths, None)), NOBODY);
    assert_eq!(error_of(token_file(&paths, Some(&mail("x@y.z")))), NOBODY);
    assert_eq!(error_of(set_default(&paths, &mail("x@y.z"))), NOBODY);
    assert!(!paths.data.exists());
}

#[test]
fn set_default_swaps_the_pointer_atomically_with_mode_0600() {
    let dir = tempfile::tempdir().unwrap();
    let paths = paths(&dir);
    place(&paths, "a@example.com", "[]");
    place(&paths, "b@example.com", "[]");
    point(&paths, "a@example.com\n");
    let held = dir.path().join("held");
    std::fs::hard_link(paths.default_account(), &held).unwrap();
    set_default(&paths, &mail("B@example.com")).unwrap();
    assert_eq!(
        std::fs::read_to_string(paths.default_account()).unwrap(),
        "b@example.com\n"
    );
    assert_eq!(std::fs::read_to_string(&held).unwrap(), "a@example.com\n");
    assert_eq!(mode(&paths.default_account()), 0o600);
    assert_eq!(names(&paths.data), ["accounts", "default"]);
}

#[test]
fn set_default_rejects_an_unknown_account_and_keeps_the_pointer() {
    let dir = tempfile::tempdir().unwrap();
    let paths = paths(&dir);
    place(&paths, "a@example.com", "[]");
    point(&paths, "a@example.com\n");
    assert_eq!(
        error_of(set_default(&paths, &mail("x@y.z"))),
        "unknown account x@y.z; known accounts: a@example.com"
    );
    assert_eq!(
        std::fs::read_to_string(paths.default_account()).unwrap(),
        "a@example.com\n"
    );
    assert_eq!(names(&paths.data), ["accounts", "default"]);
}

#[tokio::test]
async fn email_discovery_requests_drive_about_with_the_bearer() {
    let harness = Harness::start().await;
    harness.mount_about("Alice@Example.COM").await;
    assert_eq!(
        email(&harness.api).await.unwrap(),
        mail("alice@example.com")
    );
    let requests = harness.requests("/drive/v3/about").await;
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0],
        [("fields".to_owned(), "user(emailAddress)".to_owned())].into()
    );
    assert_eq!(harness.server.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn email_discovery_rejects_a_missing_address() {
    for (body, expected) in [
        (
            json!({"user": {}}),
            "cannot identify the account: google did not report an email address for this account",
        ),
        (
            json!({}),
            "cannot identify the account: google did not report an email address for this account",
        ),
        (
            json!({"user": {"emailAddress": 7}}),
            "cannot identify the account: google did not report an email address for this account",
        ),
        (
            json!({"user": {"emailAddress": "a/b@c.d"}}),
            "cannot identify the account: unsupported email address",
        ),
        (
            json!({"user": {"emailAddress": ""}}),
            "cannot identify the account: unsupported email address",
        ),
    ] {
        let harness = Harness::start().await;
        harness.mount_json("/drive/v3/about", body.clone()).await;
        assert_eq!(error_of(email(&harness.api).await), expected, "{body}");
    }
}

#[tokio::test]
async fn email_discovery_403_names_the_drive_api() {
    for status in [403, 404] {
        let harness = Harness::start().await;
        harness
            .mount_status(
                "/drive/v3/about",
                status,
                json!({"error": {"message": "Drive API has not been used"}}),
            )
            .await;
        assert_eq!(
            error_of(email(&harness.api).await),
            format!(
                "cannot identify the account: google api error {status}: Drive API has not been used (enable the Google Drive API for the project)"
            )
        );
    }
}

#[tokio::test]
async fn email_discovery_network_errors_stay_plain() {
    let harness = Harness::start().await;
    harness
        .mount_status(
            "/drive/v3/about",
            500,
            json!({"error": {"message": "backend failed"}}),
        )
        .await;
    assert_eq!(
        error_of(email(&harness.api).await),
        "cannot identify the account: google api error 500: backend failed"
    );
    let message = error_of(email(&common::unreachable().await).await);
    assert!(
        message.starts_with("cannot identify the account: request failed:"),
        "{message}"
    );
    assert!(!message.contains("Drive API"), "{message}");
}

#[tokio::test]
async fn an_expired_legacy_token_has_its_own_message() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("token.json");
    std::fs::write(&file, EXPIRED_TOKEN).unwrap();
    let client = https_builder().build(connector());
    let auth = authenticator(secret(), &file, None, false, client.clone())
        .await
        .unwrap();
    let message = auth.token(&SCOPES).await.unwrap_err().to_string();
    assert!(
        message.contains("no valid existing token: run `gdcli auth login`"),
        "{message}"
    );
    assert!(!message.contains("no valid token for"), "{message}");
    let account = mail("alice@example.com");
    let auth = authenticator(secret(), &file, Some(&account), false, client)
        .await
        .unwrap();
    let message = auth.token(&SCOPES).await.unwrap_err().to_string();
    assert!(
        message.contains(
            "no valid token for alice@example.com: run `gdcli auth login` and choose that account"
        ),
        "{message}"
    );
}

#[tokio::test]
async fn store_makes_the_first_account_the_default_only() {
    let dir = tempfile::tempdir().unwrap();
    let paths = paths(&dir);
    let harness = Harness::start().await;
    harness.mount_about("alice@example.com").await;
    create(&paths).unwrap();
    let file = pending(&paths, "first");
    let inode = std::fs::metadata(file.path()).unwrap().ino();
    let first = store(&paths, file, &harness.api).await.unwrap();
    assert_eq!(first.email, mail("alice@example.com"));
    assert!(first.added && first.default);
    assert_eq!(
        rendered(&first),
        "account: alice@example.com\nstatus: added\ndefault: true\n"
    );
    assert_eq!(
        std::fs::metadata(paths.account(&first.email))
            .unwrap()
            .ino(),
        inode
    );
    harness.server.reset().await;
    harness.mount_about("bob@example.org").await;
    let second = store(&paths, pending(&paths, "second"), &harness.api)
        .await
        .unwrap();
    assert_eq!(second.email, mail("bob@example.org"));
    assert!(second.added && !second.default);
    assert_eq!(
        rendered(&second),
        "account: bob@example.org\nstatus: added\ndefault: false\n"
    );
    assert_eq!(
        std::fs::read_to_string(paths.default_account()).unwrap(),
        "alice@example.com\n"
    );
    assert_eq!(
        std::fs::read_to_string(paths.account(&mail("alice@example.com"))).unwrap(),
        "first"
    );
    assert_eq!(
        std::fs::read_to_string(paths.account(&mail("bob@example.org"))).unwrap(),
        "second"
    );
    assert_eq!(
        names(&paths.accounts()),
        ["alice@example.com.json", "bob@example.org.json"]
    );
}

#[tokio::test]
async fn store_makes_the_first_account_default_over_a_stale_pointer() {
    let dir = tempfile::tempdir().unwrap();
    let paths = paths(&dir);
    let harness = Harness::start().await;
    harness.mount_about("alice@example.com").await;
    point(&paths, "ghost@example.com\n");
    let stored = store(&paths, pending(&paths, "t"), &harness.api)
        .await
        .unwrap();
    assert!(stored.added && stored.default);
    assert_eq!(
        std::fs::read_to_string(paths.default_account()).unwrap(),
        "alice@example.com\n"
    );
}

#[tokio::test]
async fn store_refreshes_a_known_account_and_keeps_the_default() {
    let dir = tempfile::tempdir().unwrap();
    let paths = paths(&dir);
    let harness = Harness::start().await;
    harness.mount_about("alice@example.com").await;
    place(&paths, "alice@example.com", "old");
    place(&paths, "bob@example.org", "bob");
    point(&paths, "bob@example.org\n");
    let held = dir.path().join("held");
    std::fs::hard_link(paths.account(&mail("alice@example.com")), &held).unwrap();
    let stored = store(&paths, pending(&paths, "new"), &harness.api)
        .await
        .unwrap();
    assert!(!stored.added && !stored.default);
    assert_eq!(
        rendered(&stored),
        "account: alice@example.com\nstatus: refreshed\ndefault: false\n"
    );
    assert_eq!(
        std::fs::read_to_string(paths.account(&mail("alice@example.com"))).unwrap(),
        "new"
    );
    assert_eq!(std::fs::read_to_string(&held).unwrap(), "old");
    assert_eq!(
        std::fs::read_to_string(paths.default_account()).unwrap(),
        "bob@example.org\n"
    );
    point(&paths, "alice@example.com\n");
    let stored = store(&paths, pending(&paths, "newer"), &harness.api)
        .await
        .unwrap();
    assert!(!stored.added && stored.default);
    assert_eq!(names(&paths.accounts()).len(), 2);
    std::fs::remove_file(paths.default_account()).unwrap();
    let stored = store(&paths, pending(&paths, "newest"), &harness.api)
        .await
        .unwrap();
    assert!(!stored.added && !stored.default);
    assert_eq!(
        std::fs::read_to_string(paths.account(&stored.email)).unwrap(),
        "newest"
    );
    assert!(!paths.default_account().exists());
}

#[tokio::test]
async fn store_failure_leaves_no_account_and_no_temp_file() {
    let dir = tempfile::tempdir().unwrap();
    let paths = paths(&dir);
    let harness = Harness::start().await;
    harness.fail_all(403, "{}").await;
    let result = store(&paths, pending(&paths, "t"), &harness.api).await;
    assert!(error_of(result).starts_with("cannot identify the account:"));
    assert_eq!(names(&paths.accounts()), Vec::<String>::new());
    assert!(!paths.default_account().exists());
    place(&paths, "alice@example.com", "keep");
    let result = store(&paths, pending(&paths, "t"), &harness.api).await;
    assert!(result.is_err());
    assert_eq!(names(&paths.accounts()), ["alice@example.com.json"]);
    assert_eq!(
        std::fs::read_to_string(paths.account(&mail("alice@example.com"))).unwrap(),
        "keep"
    );
}

#[tokio::test]
async fn store_creates_directories_0700_and_files_0600() {
    let dir = tempfile::tempdir().unwrap();
    let paths = paths(&dir);
    let harness = Harness::start().await;
    harness.mount_about("alice@example.com").await;
    create(&paths).unwrap();
    assert_eq!(mode(&paths.data), 0o700);
    assert_eq!(mode(&paths.accounts()), 0o700);
    let stored = store(&paths, pending(&paths, "t"), &harness.api)
        .await
        .unwrap();
    assert_eq!(mode(&paths.account(&stored.email)), 0o600);
    assert_eq!(mode(&paths.default_account()), 0o600);
    assert_eq!(mode(&paths.accounts()), 0o700);
}

#[tokio::test]
async fn adopt_moves_the_legacy_token_byte_for_byte_with_mode_0600_and_sets_the_default() {
    let dir = tempfile::tempdir().unwrap();
    let paths = paths(&dir);
    let harness = Harness::start().await;
    harness.mount_about("Alice@Example.com").await;
    let bytes = "[{\"legacy\": true}]\n";
    legacy(&paths, bytes);
    let inode = std::fs::metadata(paths.legacy_token()).unwrap().ino();
    adopt(&paths, &harness.api).await.unwrap();
    let account = paths.account(&mail("alice@example.com"));
    assert_eq!(std::fs::read_to_string(&account).unwrap(), bytes);
    assert_eq!(std::fs::metadata(&account).unwrap().ino(), inode);
    assert_eq!(mode(&account), 0o600);
    assert_eq!(mode(&paths.accounts()), 0o700);
    assert!(!paths.legacy_token().exists());
    assert_eq!(
        std::fs::read_to_string(paths.default_account()).unwrap(),
        "alice@example.com\n"
    );
    assert_eq!(mode(&paths.default_account()), 0o600);
    assert_eq!(names(&paths.data), ["accounts", "default"]);
}

#[tokio::test]
async fn adopt_overwrites_a_stale_pointer() {
    let dir = tempfile::tempdir().unwrap();
    let paths = paths(&dir);
    let harness = Harness::start().await;
    harness.mount_about("alice@example.com").await;
    point(&paths, "ghost@example.com\n");
    legacy(&paths, "[]");
    adopt(&paths, &harness.api).await.unwrap();
    assert_eq!(
        std::fs::read_to_string(paths.default_account()).unwrap(),
        "alice@example.com\n"
    );
    assert_eq!(default_account(&paths).unwrap(), mail("alice@example.com"));
}

#[tokio::test]
async fn migrate_is_skipped_when_accounts_exist() {
    let dir = tempfile::tempdir().unwrap();
    let paths = paths(&dir);
    migrate(&paths).await.unwrap();
    assert!(!paths.data.exists());
    place(&paths, "alice@example.com", "kept");
    legacy(&paths, "stray");
    migrate(&paths).await.unwrap();
    assert_eq!(
        std::fs::read_to_string(paths.legacy_token()).unwrap(),
        "stray"
    );
    assert_eq!(
        std::fs::read_to_string(paths.account(&mail("alice@example.com"))).unwrap(),
        "kept"
    );
    assert!(!paths.default_account().exists());
    let other = tempfile::tempdir().unwrap();
    let paths = self::paths(&other);
    legacy(&paths, "stray");
    let message = error_of(migrate(&paths).await);
    assert!(message.contains("cannot read client secret"), "{message}");
    assert_eq!(
        std::fs::read_to_string(paths.legacy_token()).unwrap(),
        "stray"
    );
}

#[tokio::test]
async fn adopt_failure_keeps_the_legacy_file() {
    let dir = tempfile::tempdir().unwrap();
    let paths = paths(&dir);
    let harness = Harness::start().await;
    harness.fail_all(403, "{}").await;
    legacy(&paths, "[{\"legacy\": true}]");
    let message = error_of(adopt(&paths, &harness.api).await);
    assert!(
        message.starts_with("cannot identify the account:"),
        "{message}"
    );
    assert_eq!(
        std::fs::read_to_string(paths.legacy_token()).unwrap(),
        "[{\"legacy\": true}]"
    );
    assert!(!paths.accounts().exists());
    assert!(!paths.default_account().exists());
    let other = tempfile::tempdir().unwrap();
    let paths = self::paths(&other);
    let harness = Harness::start().await;
    harness.mount_about("alice@example.com").await;
    let message = error_of(adopt(&paths, &harness.api).await);
    assert!(message.starts_with("cannot move "), "{message}");
    assert_eq!(names(&paths.accounts()), Vec::<String>::new());
}

#[tokio::test]
async fn a_second_adoption_accepts_the_moved_file() {
    let dir = tempfile::tempdir().unwrap();
    let paths = paths(&dir);
    let harness = Harness::start().await;
    harness.mount_about("alice@example.com").await;
    legacy(&paths, "[{\"legacy\": true}]");
    let (first, second) = tokio::join!(adopt(&paths, &harness.api), adopt(&paths, &harness.api));
    first.unwrap();
    second.unwrap();
    assert_eq!(names(&paths.accounts()), ["alice@example.com.json"]);
    assert_eq!(
        std::fs::read_to_string(paths.account(&mail("alice@example.com"))).unwrap(),
        "[{\"legacy\": true}]"
    );
    assert!(!paths.legacy_token().exists());
    assert_eq!(default_account(&paths).unwrap(), mail("alice@example.com"));
    assert_eq!(harness.requests("/drive/v3/about").await.len(), 2);
}
