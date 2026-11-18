use crate::api::Api;
use crate::cli::DocsCommand;
use anyhow::Result;
use google_apis_common::Connector;
use http_body_util::BodyExt;
use std::io::Write;

const MARKDOWN: &str = "text/markdown";

pub fn document_id(input: &str) -> &str {
    match input.split_once("/d/") {
        Some((_, rest)) => rest.split(['/', '?', '#']).next().unwrap_or(rest),
        None => input,
    }
}

pub async fn get<C: Connector>(api: &Api<C>, id: &str, out: &mut impl Write) -> Result<()> {
    let response = api
        .drive
        .files()
        .export(document_id(id), MARKDOWN)
        .doit()
        .await?;
    let mut body = response.into_body();
    while let Some(frame) = body.frame().await {
        if let Ok(data) = frame?.into_data() {
            out.write_all(&data)?;
        }
    }
    Ok(())
}

pub async fn run<C: Connector>(
    api: &Api<C>,
    command: DocsCommand,
    out: &mut impl Write,
) -> Result<()> {
    match command {
        DocsCommand::Get { id } => get(api, &id, out).await,
    }
}
