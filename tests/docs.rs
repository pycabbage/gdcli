mod common;

use common::{Harness, text};
use gdcli::docs;
use serde_json::json;
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
        docs::get(&harness.api, input, &mut out).await.unwrap();
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
    let error = docs::get(&harness.api, "BIG", &mut out).await.unwrap_err();
    assert!(gdcli::message(&error).contains("too large to be exported"));
    assert_eq!(out, b"");
}
