use gdcli::Paths;
use gdcli::auth::browser::{Launch, Terminal};
use gdcli::auth::setup::{setup, typed_path};
use gdcli::auth::{install_client_secret, validate_client_secret};
use std::io::Cursor;
use std::os::unix::fs::PermissionsExt;
use std::time::Duration;

const INSTALLED: &str = r#"{"installed":{"client_id":"id.apps.googleusercontent.com","client_secret":"s","auth_uri":"https://accounts.google.com/o/oauth2/auth","token_uri":"https://oauth2.googleapis.com/token","redirect_uris":["http://localhost"]}}"#;
const WEB: &str = r#"{"web":{"client_id":"id","client_secret":"s","auth_uri":"a","token_uri":"t","redirect_uris":[]}}"#;

fn quiet() -> Terminal<Vec<u8>, impl Fn(&str) -> std::io::Result<()> + Send + Sync + 'static> {
    Terminal::new(Launch::Skip, Vec::new(), |_: &str| Ok(()))
}

fn mode(path: &std::path::Path) -> u32 {
    std::fs::metadata(path).unwrap().permissions().mode() & 0o777
}

async fn bounded<T>(future: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), future)
        .await
        .expect("setup must fail before reaching the login flow")
}

#[test]
fn typed_paths_are_trimmed_unquoted_and_home_expanded() {
    let home = std::path::Path::new("/home/me");
    assert_eq!(
        typed_path("  /a/b.json \n", Some(home)),
        std::path::Path::new("/a/b.json")
    );
    assert_eq!(
        typed_path("\"/a/my file.json\"", Some(home)),
        std::path::Path::new("/a/my file.json")
    );
    assert_eq!(
        typed_path("'~/Downloads/x.json'", Some(home)),
        std::path::Path::new("/home/me/Downloads/x.json")
    );
    assert_eq!(
        typed_path("~/x.json", None),
        std::path::Path::new("~/x.json")
    );
    assert_eq!(
        typed_path("~x.json", Some(home)),
        std::path::Path::new("~x.json")
    );
    assert!(typed_path(" \n", Some(home)).as_os_str().is_empty());
}

#[test]
fn accepts_only_installed_app_secrets() {
    assert!(validate_client_secret(INSTALLED.as_bytes()).is_ok());
    assert!(validate_client_secret(WEB.as_bytes()).is_err());
    assert!(validate_client_secret(b"{}").is_err());
    assert!(validate_client_secret(b"not json").is_err());
    assert!(validate_client_secret(br#"{"installed":{"client_id":"x"}}"#).is_err());
}

#[test]
fn install_copies_verbatim_with_mode_0600_and_creates_directories() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("download.json");
    std::fs::write(&source, INSTALLED).unwrap();
    std::fs::set_permissions(&source, std::fs::Permissions::from_mode(0o644)).unwrap();
    let destination = dir.path().join("config/gdcli/client_secret.json");
    install_client_secret(&source, &destination).unwrap();
    assert_eq!(std::fs::read_to_string(&destination).unwrap(), INSTALLED);
    assert_eq!(mode(&destination), 0o600);
    assert_eq!(mode(destination.parent().unwrap()), 0o700);
}

#[test]
fn install_tightens_an_existing_world_readable_file() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("download.json");
    let destination = dir.path().join("client_secret.json");
    std::fs::write(&source, INSTALLED).unwrap();
    std::fs::write(&destination, "old old old old old old old old old old").unwrap();
    std::fs::set_permissions(&destination, std::fs::Permissions::from_mode(0o666)).unwrap();
    install_client_secret(&source, &destination).unwrap();
    assert_eq!(std::fs::read_to_string(&destination).unwrap(), INSTALLED);
    assert_eq!(mode(&destination), 0o600);
}

#[test]
fn install_rejects_missing_and_invalid_sources_without_touching_the_destination() {
    let dir = tempfile::tempdir().unwrap();
    let destination = dir.path().join("client_secret.json");
    let missing = install_client_secret(&dir.path().join("nope.json"), &destination).unwrap_err();
    assert!(gdcli::message(&missing).contains("nope.json"));
    let invalid = dir.path().join("web.json");
    std::fs::write(&invalid, WEB).unwrap();
    let error = install_client_secret(&invalid, &destination).unwrap_err();
    assert!(gdcli::message(&error).contains("Desktop app"));
    assert!(!destination.exists());
}

#[tokio::test]
async fn setup_prints_instructions_destination_and_rejects_a_bad_path() {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::new(dir.path().join("config"), dir.path().join("data"));
    let mut out = Vec::new();
    let error = bounded(setup(
        &paths,
        Cursor::new("/does/not/exist.json\n"),
        &mut out,
        quiet(),
    ))
    .await
    .unwrap_err();
    let printed = String::from_utf8(out).unwrap();
    assert!(printed.contains("Google Calendar API"));
    assert!(printed.contains(&format!("Destination: {}", paths.client_secret().display())));
    assert!(gdcli::message(&error).contains("/does/not/exist.json"));
    assert!(!paths.client_secret().exists());
}

#[tokio::test]
async fn setup_with_empty_answer_requires_a_valid_placed_secret() {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::new(dir.path().join("config"), dir.path().join("data"));
    let error = bounded(setup(&paths, Cursor::new("\n"), &mut Vec::new(), quiet()))
        .await
        .unwrap_err();
    assert!(gdcli::message(&error).contains("client_secret.json"));
    std::fs::create_dir_all(&paths.config).unwrap();
    std::fs::write(paths.client_secret(), WEB).unwrap();
    let error = bounded(setup(&paths, Cursor::new("\n"), &mut Vec::new(), quiet()))
        .await
        .unwrap_err();
    assert!(gdcli::message(&error).contains("Desktop app"));
}
