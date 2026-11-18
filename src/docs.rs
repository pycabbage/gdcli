use crate::events::text;
use crate::google::{ApiError, Google};
use crate::output::{compact, record};
use anyhow::{Result, bail, ensure};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use std::io::Write;

const DOCUMENT: &str = "application/vnd.google-apps.document";
const TAB: &str = "tabProperties(tabId,title,index,parentTabId,nestingLevel)";
const FILE_FIELDS: &str = "id,name,mimeType,createdTime,modifiedTime,version,webViewLink,owners(displayName,emailAddress),lastModifyingUser(displayName,emailAddress),shared,trashed,starred,viewedByMeTime,modifiedByMeTime,parents,description";

pub fn document_id(input: &str) -> &str {
    match input.split_once("/d/") {
        Some((_, rest)) => rest.split(['/', '?', '#']).next().unwrap_or(rest),
        None => input,
    }
}

fn flatten(tabs: Option<&Value>, found: &mut Vec<Value>) {
    for tab in tabs.and_then(Value::as_array).into_iter().flatten() {
        let properties = &tab["tabProperties"];
        found.push(json!({
            "id": text(properties, "/tabId"),
            "title": text(properties, "/title"),
            "index": properties["index"].as_u64().unwrap_or(0),
            "parent_id": text(properties, "/parentTabId"),
            "nesting_level": properties["nestingLevel"].as_u64().unwrap_or(0),
        }));
        flatten(tab.get("childTabs"), found);
    }
}

pub async fn tabs(google: &Google, id: &str) -> Result<Vec<Value>> {
    let fields = format!("tabs({TAB},childTabs({TAB},childTabs({TAB})))");
    let query = [("includeTabsContent", "true"), ("fields", &fields)];
    let document = google
        .json(&google.docs_api, &["documents", id], &query)
        .await?;
    let mut found = Vec::new();
    flatten(document.get("tabs"), &mut found);
    Ok(found)
}

fn path(tabs: &[Value], tab: &Value) -> String {
    let (title, parent) = (text(tab, "/title"), text(tab, "/parent_id"));
    match tabs.iter().find(|other| text(other, "/id") == parent) {
        Some(parent) if !text(tab, "/parent_id").is_empty() => {
            format!("{} > {title}", path(tabs, parent))
        }
        _ => title.to_owned(),
    }
}

pub fn resolve_tab(tabs: &[Value], wanted: &str) -> Result<String> {
    let lower = wanted.to_lowercase();
    for step in 0..4 {
        let hits: Vec<&Value> = tabs
            .iter()
            .filter(|tab| {
                let (id, title) = (text(tab, "/id"), text(tab, "/title"));
                match step {
                    0 => id == wanted,
                    1 => !wanted.starts_with("t.") && id.strip_prefix("t.") == Some(wanted),
                    2 => title == wanted,
                    _ => title.to_lowercase() == lower,
                }
            })
            .collect();
        match hits.as_slice() {
            [] => {}
            [tab] => return Ok(text(tab, "/id").to_owned()),
            _ => {
                let names: Vec<String> = hits
                    .iter()
                    .map(|tab| format!("{} ({})", text(tab, "/id"), path(tabs, tab)))
                    .collect();
                bail!("tab \"{wanted}\" is ambiguous: {}", names.join(", "));
            }
        }
    }
    let names: Vec<String> = tabs
        .iter()
        .map(|tab| format!("\"{}\" ({})", text(tab, "/title"), text(tab, "/id")))
        .collect();
    bail!(
        "tab \"{wanted}\" not found; available: {}",
        names.join(", ")
    )
}

pub async fn get(
    google: &Google,
    input: &str,
    tab: Option<&str>,
    out: &mut impl Write,
) -> Result<()> {
    let id = document_id(input);
    let response = match tab {
        None => {
            let query = [("mimeType", "text/markdown")];
            google
                .send(&google.drive, &["files", id, "export"], &query)
                .await?
        }
        Some(wanted) => {
            let tab_id = resolve_tab(&tabs(google, id).await?, wanted)?;
            google.export_tab(id, &tab_id).await?
        }
    };
    let mut body = response.into_body();
    while let Some(frame) = body.frame().await {
        if let Ok(data) = frame?.into_data() {
            out.write_all(&data)?;
        }
    }
    Ok(())
}

fn person(value: &Value) -> Value {
    json!({"name": text(value, "/displayName"), "email": text(value, "/emailAddress")})
}

pub async fn metadata(
    google: &Google,
    input: &str,
    json: bool,
    out: &mut impl Write,
) -> Result<()> {
    let id = document_id(input);
    let query = [("supportsAllDrives", "true"), ("fields", FILE_FIELDS)];
    let segments = ["files", id];
    let (file, tabs) = tokio::join!(
        google.json(&google.drive, &segments, &query),
        tabs(google, id)
    );
    let file = file?;
    let kind = text(&file, "/mimeType");
    ensure!(kind == DOCUMENT, "not a google doc (mimeType {kind})");
    let (tabs, unavailable) = match tabs {
        Ok(tabs) => (tabs, None),
        Err(error) => {
            let reason = error
                .downcast_ref::<ApiError>()
                .and_then(|api| api.message.clone());
            (
                Vec::new(),
                Some(reason.unwrap_or_else(|| format!("{error:#}"))),
            )
        }
    };
    let owners: Value = file["owners"]
        .as_array()
        .into_iter()
        .flatten()
        .map(person)
        .collect();
    let entry = compact(json!({
        "id": text(&file, "/id"),
        "title": text(&file, "/name"),
        "modified_time": text(&file, "/modifiedTime"),
        "created_time": text(&file, "/createdTime"),
        "version": text(&file, "/version"),
        "owners": owners,
        "last_modified_by": person(&file["lastModifyingUser"]),
        "html_link": text(&file, "/webViewLink"),
        "shared": file["shared"],
        "trashed": file["trashed"],
        "starred": file["starred"],
        "viewed_by_me_time": text(&file, "/viewedByMeTime"),
        "modified_by_me_time": text(&file, "/modifiedByMeTime"),
        "parents": file["parents"],
        "description": text(&file, "/description"),
        "mime_type": kind,
        "tabs": tabs,
        "tabs_unavailable": unavailable,
    }))
    .unwrap_or_default();
    Ok(record(out, &entry, json)?)
}
