use super::records::{ARTIFACTS, Artifact, artifacts, conference_records, event, items, record_id};
use crate::cli::ReadArgs;
use crate::docs;
use crate::events::text;
use crate::google::Google;
use anyhow::{Context, Result, bail, ensure};
use std::io::Write;

const NOTES: (&str, &Artifact) = ("gemini notes document", &ARTIFACTS[3]);
const TRANSCRIPT: (&str, &Artifact) = ("transcript", &ARTIFACTS[2]);

pub async fn notes(google: &Google, args: &ReadArgs, out: &mut impl Write) -> Result<()> {
    read(google, &args.id, NOTES, args.tab.as_deref(), out).await
}

pub async fn transcript(google: &Google, args: &ReadArgs, out: &mut impl Write) -> Result<()> {
    read(google, &args.id, TRANSCRIPT, args.tab.as_deref(), out).await
}

async fn read(
    google: &Google,
    id: &str,
    (label, spec): (&str, &Artifact),
    tab: Option<&str>,
    out: &mut impl Write,
) -> Result<()> {
    let (event, code) = event(google, id).await?;
    let records = conference_records(google, &event, &code).await?;
    let entries = items(&records, "conferenceRecords");
    ensure!(
        !entries.is_empty(),
        "no conference record for event {id}; Meet keeps a record only from the start of a meeting until 30 days after it (see the attachment lines of calendar get)"
    );
    let (mut found, mut pending) = (Vec::new(), None);
    for entry in entries.iter().rev() {
        let page = artifacts(google, record_id(entry), spec).await?;
        for item in items(&page, spec.0) {
            let item = spec.4(item);
            if text(&item, "/state") == "FILE_GENERATED" && !text(&item, "/document_id").is_empty()
            {
                found.push(item);
            } else {
                pending = Some(text(&item, "/state").to_owned());
            }
        }
    }
    if found.is_empty() {
        match pending {
            Some(state) => bail!("the {label} of event {id} is not ready (state {state})"),
            None => bail!("no {label} for event {id}"),
        }
    }
    let mut responses = Vec::with_capacity(found.len());
    for item in &found {
        let document = text(item, "/document_id");
        let response = docs::response(google, document, tab)
            .await
            .with_context(|| format!("cannot read {label} {document}"))?;
        responses.push(response);
    }
    let several = responses.len() > 1;
    for (item, response) in found.iter().zip(responses) {
        if several {
            let (document, start, end) = (
                text(item, "/document_id"),
                text(item, "/start"),
                text(item, "/end"),
            );
            writeln!(out, "document: id={document} start={start} end={end}")?;
        }
        docs::copy(response, out).await?;
        if several {
            out.write_all(b"\n\n")?;
        }
    }
    Ok(())
}
