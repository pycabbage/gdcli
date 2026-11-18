use crate::google::Google;
use anyhow::Result;
use http_body_util::BodyExt;
use std::io::Write;

pub fn document_id(input: &str) -> &str {
    match input.split_once("/d/") {
        Some((_, rest)) => rest.split(['/', '?', '#']).next().unwrap_or(rest),
        None => input,
    }
}

pub async fn get(google: &Google, id: &str, out: &mut impl Write) -> Result<()> {
    let response = google
        .send(
            &google.drive,
            &["files", document_id(id), "export"],
            &[("mimeType", "text/markdown")],
        )
        .await?;
    let mut body = response.into_body();
    while let Some(frame) = body.frame().await {
        if let Ok(data) = frame?.into_data() {
            out.write_all(&data)?;
        }
    }
    Ok(())
}
