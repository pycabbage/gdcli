mod common;

use common::{Harness, text};
use gdcli::docs;
use serde_json::{Value, json};
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

#[tokio::test]
async fn streams_markdown_for_bare_ids_and_urls() {
    let harness = Harness::start().await;
    let markdown = "# Title\n\nbody line\n".repeat(50_000);
    Mock::given(method("GET"))
        .and(header("authorization", common::BEARER))
        .and(path("/drive/v3/files/DOC123/export"))
        .and(query_param("mimeType", "text/markdown"))
        .respond_with(ResponseTemplate::new(200).set_body_string(markdown.clone()))
        .mount(&harness.server)
        .await;
    for input in [
        "DOC123",
        "https://docs.google.com/document/d/DOC123/edit?tab=t.0",
        "https://docs.google.com/document/d/DOC123",
        "https://docs.google.com/document/d/DOC123?usp=sharing",
        "https://docs.google.com/document/d/DOC123/edit#heading=h.1",
    ] {
        let mut out = Vec::new();
        docs::get(&harness.api, input, None, &mut out)
            .await
            .unwrap();
        assert_eq!(text(out), markdown, "{input}");
    }
}

#[tokio::test]
async fn reports_export_failures() {
    let harness = Harness::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(403).set_body_json(json!({"error": {"errors": [{"reason": "exportSizeLimitExceeded"}], "message": "This file is too large to be exported."}})))
        .mount(&harness.server)
        .await;
    let mut out = Vec::new();
    let error = docs::get(&harness.api, "BIG", None, &mut out)
        .await
        .unwrap_err();
    assert!(gdcli::message(&error).contains("too large to be exported"));
    assert_eq!(out, b"");
}

const TAB_FIELDS: &str = "tabs(tabProperties(tabId,title,index,parentTabId,nestingLevel),childTabs(tabProperties(tabId,title,index,parentTabId,nestingLevel),childTabs(tabProperties(tabId,title,index,parentTabId,nestingLevel))))";

fn tab(id: &str, title: &str, index: u32, parent: Option<(&str, u32)>) -> Value {
    let mut properties = json!({"tabId": id, "title": title});
    if index > 0 {
        properties["index"] = json!(index);
    }
    if let Some((parent, level)) = parent {
        properties["parentTabId"] = json!(parent);
        properties["nestingLevel"] = json!(level);
    }
    json!({ "tabProperties": properties })
}

fn tab_tree() -> Value {
    let mut first = tab("t.first", "Overview", 0, None);
    first["childTabs"] = json!([tab("t.child", "Notes", 0, Some(("t.first", 1)))]);
    json!({"tabs": [first, tab("t.second", "Notes", 1, None), tab("t.third", "Plan", 2, None)]})
}

async fn mount_tabs(harness: &Harness, body: Value) {
    Mock::given(method("GET"))
        .and(header("authorization", common::BEARER))
        .and(path("/v1/documents/DOC123"))
        .and(query_param("includeTabsContent", "true"))
        .and(query_param("fields", TAB_FIELDS))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(&harness.server)
        .await;
}

async fn tab_ids(harness: &Harness) -> Vec<String> {
    docs::tabs(&harness.api, "DOC123")
        .await
        .unwrap()
        .iter()
        .map(|tab| tab["id"].as_str().unwrap().to_owned())
        .collect()
}

#[tokio::test]
async fn the_tab_tree_is_flattened_with_parents_and_levels() {
    let harness = Harness::start().await;
    mount_tabs(&harness, tab_tree()).await;
    let tabs = docs::tabs(&harness.api, "DOC123").await.unwrap();
    assert_eq!(
        Value::Array(tabs),
        json!([
            {"id": "t.first", "title": "Overview", "index": 0, "parent_id": "", "nesting_level": 0},
            {"id": "t.child", "title": "Notes", "index": 0, "parent_id": "t.first", "nesting_level": 1},
            {"id": "t.second", "title": "Notes", "index": 1, "parent_id": "", "nesting_level": 0},
            {"id": "t.third", "title": "Plan", "index": 2, "parent_id": "", "nesting_level": 0}
        ])
    );
    assert_eq!(tab_ids(&harness).await.len(), 4);
}

fn tabs_for_resolution() -> Vec<Value> {
    let tabs = |entries: &[(&str, &str, &str)]| -> Vec<Value> {
        entries
            .iter()
            .map(|(id, title, parent)| json!({"id": id, "title": title, "parent_id": parent}))
            .collect()
    };
    tabs(&[
        ("t.aaa", "Overview", ""),
        ("t.bbb", "Notes", ""),
        ("t.ccc", "Notes", "t.aaa"),
        ("t.ddd", "plan", ""),
        ("t.eee", "PLAN", ""),
        ("t.fff", "Budget", ""),
        ("t.ggg", "t.aaa", ""),
    ])
}

#[test]
fn tabs_resolve_by_id_then_prefixed_id_then_title_then_lowercase_title() {
    let tabs = tabs_for_resolution();
    let resolve =
        |wanted: &str| docs::resolve_tab(&tabs, wanted).map_err(|error| gdcli::message(&error));
    assert_eq!(resolve("t.aaa").unwrap(), "t.aaa");
    assert_eq!(resolve("bbb").unwrap(), "t.bbb");
    assert_eq!(resolve("Overview").unwrap(), "t.aaa");
    assert_eq!(resolve("budget").unwrap(), "t.fff");
    assert_eq!(resolve("PLAN").unwrap(), "t.eee");
    assert_eq!(resolve("plan").unwrap(), "t.ddd");
    assert_eq!(resolve("t.ggg").unwrap(), "t.ggg");
}

#[test]
fn ambiguous_and_unknown_tabs_are_errors_that_list_the_candidates() {
    let tabs = tabs_for_resolution();
    let message = |wanted: &str| gdcli::message(&docs::resolve_tab(&tabs, wanted).unwrap_err());
    assert_eq!(
        message("Notes"),
        "tab \"Notes\" is ambiguous: t.bbb (Notes), t.ccc (Overview > Notes)"
    );
    assert_eq!(
        message("pLaN"),
        "tab \"pLaN\" is ambiguous: t.ddd (plan), t.eee (PLAN)"
    );
    assert_eq!(
        message("nope"),
        "tab \"nope\" not found; available: \"Overview\" (t.aaa), \"Notes\" (t.bbb), \"Notes\" (t.ccc), \"plan\" (t.ddd), \"PLAN\" (t.eee), \"Budget\" (t.fff), \"t.aaa\" (t.ggg)"
    );
    assert!(
        gdcli::message(&docs::resolve_tab(&[], "x").unwrap_err()).contains("not found; available:")
    );
}

async fn mount_export(harness: &Harness, tab: &str, redirect: Option<&str>, content_type: &str) {
    let target = redirect.map_or_else(
        || format!("{}/blob/{tab}?dat=secret", harness.server.uri()),
        str::to_owned,
    );
    Mock::given(method("GET"))
        .and(header("authorization", common::BEARER))
        .and(path("/document/d/DOC123/export"))
        .and(query_param("format", "md"))
        .and(query_param("tab", tab))
        .respond_with(ResponseTemplate::new(307).insert_header("location", target.as_str()))
        .mount(&harness.server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/blob/{tab}")))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(format!("# markdown of {tab}\n"), content_type),
        )
        .mount(&harness.server)
        .await;
}

async fn get_tab(harness: &Harness, tab: &str) -> anyhow::Result<String> {
    let mut out = Vec::new();
    docs::get(
        &harness.api,
        "https://docs.google.com/document/d/DOC123/edit",
        Some(tab),
        &mut out,
    )
    .await?;
    Ok(text(out))
}

#[tokio::test]
async fn a_tab_is_exported_through_the_redirect_without_the_bearer_token() {
    let harness = Harness::start().await;
    mount_tabs(&harness, tab_tree()).await;
    mount_export(&harness, "t.third", None, "text/x-markdown; charset=utf-8").await;
    assert_eq!(
        get_tab(&harness, "Plan").await.unwrap(),
        "# markdown of t.third\n"
    );
    assert_eq!(
        get_tab(&harness, "third").await.unwrap(),
        "# markdown of t.third\n"
    );
    let requests = harness.server.received_requests().await.unwrap();
    for request in &requests {
        let authorized = request.headers.get("authorization").is_some();
        assert_eq!(
            authorized,
            !request.url.path().starts_with("/blob/"),
            "{}",
            request.url
        );
    }
    assert_eq!(
        requests
            .iter()
            .filter(|request| request.url.path().starts_with("/blob/"))
            .count(),
        2
    );
    assert_eq!(harness.requests("/v1/documents/DOC123").await.len(), 2);
}

#[tokio::test]
async fn an_unknown_or_ambiguous_tab_never_reaches_the_export() {
    let harness = Harness::start().await;
    mount_tabs(&harness, tab_tree()).await;
    mount_export(&harness, "t.second", None, "text/markdown").await;
    for (wanted, expected) in [
        ("Notes", "ambiguous"),
        ("missing", "not found"),
        ("", "not found"),
    ] {
        let error = get_tab(&harness, wanted).await.unwrap_err();
        assert!(gdcli::message(&error).contains(expected), "{wanted}");
    }
    assert_eq!(harness.requests("/document/d/DOC123/export").await.len(), 0);
}

#[tokio::test]
async fn redirects_to_other_hosts_and_other_content_types_are_rejected() {
    let harness = Harness::start().await;
    mount_tabs(&harness, tab_tree()).await;
    mount_export(
        &harness,
        "t.third",
        Some("http://accounts.example.com/login"),
        "text/markdown",
    )
    .await;
    let error = get_tab(&harness, "t.third").await.unwrap_err();
    assert_eq!(
        gdcli::message(&error),
        "export redirected to an unexpected host"
    );
    let harness = Harness::start().await;
    mount_tabs(&harness, tab_tree()).await;
    mount_export(&harness, "t.third", None, "text/html").await;
    let error = get_tab(&harness, "t.third").await.unwrap_err();
    assert_eq!(gdcli::message(&error), "unexpected export content type");
}

#[tokio::test]
async fn a_throttled_export_is_a_short_error() {
    let harness = Harness::start().await;
    mount_tabs(&harness, tab_tree()).await;
    Mock::given(method("GET"))
        .and(path("/document/d/DOC123/export"))
        .respond_with(ResponseTemplate::new(429).set_body_string("<html>too many requests</html>"))
        .mount(&harness.server)
        .await;
    let error = get_tab(&harness, "t.third").await.unwrap_err();
    assert_eq!(
        gdcli::message(&error),
        "google api error 429 Too Many Requests"
    );
}

const FILE_FIELDS: &str = "id,name,mimeType,createdTime,modifiedTime,version,webViewLink,owners(displayName,emailAddress),lastModifyingUser(displayName,emailAddress),shared,trashed,starred,viewedByMeTime,modifiedByMeTime,parents,description";

async fn mount_file(harness: &Harness, mime: &str) {
    Mock::given(method("GET"))
        .and(header("authorization", common::BEARER))
        .and(path("/drive/v3/files/DOC123"))
        .and(query_param("supportsAllDrives", "true"))
        .and(query_param("fields", FILE_FIELDS))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "DOC123", "name": "Quarterly plan", "mimeType": mime, "version": "42",
            "createdTime": "2026-10-01T00:00:00.000Z", "modifiedTime": "2026-11-17T10:30:00.123Z",
            "webViewLink": "https://docs.google.com/document/d/DOC123/edit",
            "owners": [{"displayName": "Alice", "emailAddress": "alice@example.com"}],
            "lastModifyingUser": {"displayName": "Bob", "emailAddress": "bob@example.com"},
            "shared": true, "trashed": false, "starred": false,
            "viewedByMeTime": "2026-11-18T01:00:00.000Z", "parents": ["FOLDER1"], "size": "dropped"
        })))
        .mount(&harness.server)
        .await;
}

async fn metadata(harness: &Harness, json: bool) -> anyhow::Result<String> {
    let mut out = Vec::new();
    docs::metadata(
        &harness.api,
        "https://docs.google.com/document/d/DOC123/edit",
        json,
        &mut out,
    )
    .await?;
    Ok(text(out))
}

#[tokio::test]
async fn metadata_is_one_record_in_text_and_json() {
    let harness = Harness::start().await;
    mount_file(&harness, "application/vnd.google-apps.document").await;
    mount_tabs(&harness, tab_tree()).await;
    let json = metadata(&harness, true).await.unwrap();
    assert_eq!(
        json,
        format!(
            "{}\n",
            json!({
                "id": "DOC123", "title": "Quarterly plan",
                "modified_time": "2026-11-17T10:30:00.123Z", "created_time": "2026-10-01T00:00:00.000Z",
                "version": "42",
                "owners": [{"name": "Alice", "email": "alice@example.com"}],
                "last_modified_by": {"name": "Bob", "email": "bob@example.com"},
                "html_link": "https://docs.google.com/document/d/DOC123/edit",
                "shared": true, "trashed": false, "starred": false,
                "viewed_by_me_time": "2026-11-18T01:00:00.000Z",
                "parents": ["FOLDER1"],
                "mime_type": "application/vnd.google-apps.document",
                "tabs": [
                    {"id": "t.first", "title": "Overview", "index": 0, "nesting_level": 0},
                    {"id": "t.child", "title": "Notes", "index": 0, "parent_id": "t.first", "nesting_level": 1},
                    {"id": "t.second", "title": "Notes", "index": 1, "nesting_level": 0},
                    {"id": "t.third", "title": "Plan", "index": 2, "nesting_level": 0}
                ]
            })
        )
    );
}

#[tokio::test]
async fn metadata_text_carries_the_same_information() {
    let harness = Harness::start().await;
    mount_file(&harness, "application/vnd.google-apps.document").await;
    mount_tabs(&harness, tab_tree()).await;
    let text = metadata(&harness, false).await.unwrap();
    assert_eq!(
        text,
        "id: DOC123\ntitle: Quarterly plan\nmodified_time: 2026-11-17T10:30:00.123Z\n\
         created_time: 2026-10-01T00:00:00.000Z\nversion: 42\n\
         owner: name=Alice email=alice@example.com\nlast_modified_by: name=Bob email=bob@example.com\n\
         html_link: https://docs.google.com/document/d/DOC123/edit\n\
         shared: true\ntrashed: false\nstarred: false\n\
         viewed_by_me_time: 2026-11-18T01:00:00.000Z\nparent: FOLDER1\n\
         mime_type: application/vnd.google-apps.document\n\
         tab: id=t.first title=Overview index=0 nesting_level=0\n\
         tab: id=t.child title=Notes index=0 parent_id=t.first nesting_level=1\n\
         tab: id=t.second title=Notes index=1 nesting_level=0\n\
         tab: id=t.third title=Plan index=2 nesting_level=0\n"
    );
    common::assert_same_information(&text, &metadata(&harness, true).await.unwrap());
}

#[tokio::test]
async fn metadata_rejects_files_that_are_not_google_docs() {
    let harness = Harness::start().await;
    mount_file(&harness, "application/pdf").await;
    mount_tabs(&harness, tab_tree()).await;
    let error = metadata(&harness, false).await.unwrap_err();
    assert_eq!(
        gdcli::message(&error),
        "not a google doc (mimeType application/pdf)"
    );
}

#[tokio::test]
async fn metadata_survives_an_unavailable_docs_api_with_one_line() {
    let harness = Harness::start().await;
    mount_file(&harness, "application/vnd.google-apps.document").await;
    harness
        .mount_status(
            "/v1/documents/DOC123",
            403,
            json!({"error": {"code": 403, "message": "Google Docs API has not been used in project 1 before"}}),
        )
        .await;
    let text = metadata(&harness, false).await.unwrap();
    assert!(
        text.ends_with("tabs_unavailable: Google Docs API has not been used in project 1 before\n"),
        "{text}"
    );
    assert!(!text.contains("tab:"));
    let json: Value = serde_json::from_str(&metadata(&harness, true).await.unwrap()).unwrap();
    assert_eq!(
        json["tabs_unavailable"],
        "Google Docs API has not been used in project 1 before"
    );
    assert!(json.get("tabs").is_none());
    assert_eq!(json["title"], "Quarterly plan");
}

#[tokio::test]
async fn metadata_fails_when_drive_does() {
    let harness = Harness::start().await;
    harness
        .mount_status(
            "/drive/v3/files/DOC123",
            404,
            json!({"error": {"code": 404, "message": "File not found: DOC123."}}),
        )
        .await;
    mount_tabs(&harness, tab_tree()).await;
    let error = metadata(&harness, false).await.unwrap_err();
    assert_eq!(
        gdcli::message(&error),
        "google api error 404: File not found: DOC123."
    );
}

#[tokio::test]
async fn both_markdown_content_types_are_accepted_and_everything_else_is_rejected() {
    for (kind, accepted) in [
        ("text/markdown", true),
        ("text/markdown; charset=utf-8", true),
        ("text/x-markdown", true),
        ("text/x-markdown; charset=utf-8", true),
        ("text/html; charset=utf-8", false),
        ("text/plain", false),
        ("application/json", false),
        ("text/x-mark", false),
        ("Text/X-Markdown; Charset=UTF-8", true),
        ("TEXT/MARKDOWN", true),
        ("Text/HTML", false),
    ] {
        let harness = Harness::start().await;
        mount_tabs(&harness, tab_tree()).await;
        mount_export(&harness, "t.third", None, kind).await;
        let result = get_tab(&harness, "t.third").await;
        assert_eq!(result.is_ok(), accepted, "{kind}");
        if let Err(error) = result {
            assert_eq!(gdcli::message(&error), "unexpected export content type");
        }
    }
}

#[tokio::test]
async fn a_second_redirect_is_an_error_and_is_not_followed() {
    let harness = Harness::start().await;
    mount_tabs(&harness, tab_tree()).await;
    let blob = format!("{}/blob/t.third", harness.server.uri());
    Mock::given(method("GET"))
        .and(path("/document/d/DOC123/export"))
        .respond_with(ResponseTemplate::new(307).insert_header("location", blob.as_str()))
        .mount(&harness.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/blob/t.third"))
        .respond_with(ResponseTemplate::new(307).insert_header(
            "location",
            format!("{}/again", harness.server.uri()).as_str(),
        ))
        .mount(&harness.server)
        .await;
    let error = get_tab(&harness, "t.third").await.unwrap_err();
    assert_eq!(
        gdcli::message(&error),
        "google api error 307 Temporary Redirect"
    );
    assert_eq!(harness.requests("/again").await.len(), 0);
}

#[test]
fn the_t_prefix_is_only_added_to_arguments_that_lack_it() {
    let tabs: Vec<Value> = vec![
        json!({"id": "t.t.zzz", "title": "Q", "parent_id": ""}),
        json!({"id": "t.abc", "title": "R", "parent_id": ""}),
    ];
    assert_eq!(docs::resolve_tab(&tabs, "abc").unwrap(), "t.abc");
    assert!(docs::resolve_tab(&tabs, "t.zzz").is_err());
    assert_eq!(docs::resolve_tab(&tabs, "t.t.zzz").unwrap(), "t.t.zzz");
}

#[test]
fn only_https_hosts_below_googleusercontent_com_may_receive_the_export_redirect() {
    let base = url::Url::parse("https://docs.google.com/document/d/D/export").unwrap();
    let allowed = |location: &str| gdcli::google::googleusercontent(&base.join(location).unwrap());
    for ok in [
        "https://doc-0k-0k-docstext.googleusercontent.com/x",
        "https://DOC-0K.GoogleUserContent.COM/x",
        "https://x@doc.googleusercontent.com/",
    ] {
        assert!(allowed(ok), "{ok}");
    }
    for bad in [
        "https://evilgoogleusercontent.com/",
        "https://googleusercontent.com.evil.com/",
        "https://googleusercontent.com/",
        "http://doc.googleusercontent.com/",
        "https://doc.googleusercontent.com./x",
        "//evil.com/x",
        "/x",
        "x",
        "https://doc.googleusercontent.com@evil.com/",
        "https://evil.com#.googleusercontent.com",
        "https://evil.com/?h=.googleusercontent.com",
        "https://accounts.google.com/ServiceLogin",
    ] {
        assert!(!allowed(bad), "{bad}");
    }
}
