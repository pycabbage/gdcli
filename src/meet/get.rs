use super::model::{Artifact, ConferenceRecord, Participant, Person, Space};
use crate::api::Api;
use crate::output::{field, field_opt};
use anyhow::Result;
use google_apis_common::Connector;
use std::io::Write;

const MEET_URL: &str = "https://meet.google.com/";
const RECORD_PREFIX: &str = "conferenceRecords/";
const RECENT_RECORDS: u32 = 5;

#[derive(Debug, PartialEq, Eq)]
pub enum Target {
    Code(String),
    Record(String),
}

fn is_word(part: &str, length: usize) -> bool {
    part.len() == length && part.bytes().all(|byte| byte.is_ascii_lowercase())
}

pub fn is_meeting_code(value: &str) -> bool {
    let mut parts = value.split('-');
    match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some(a), Some(b), Some(c), None) => is_word(a, 3) && is_word(b, 4) && is_word(c, 3),
        _ => false,
    }
}

pub fn record_id(name: &str) -> &str {
    name.strip_prefix(RECORD_PREFIX).unwrap_or(name)
}

pub fn parse_target(input: &str) -> Target {
    let input = input.strip_prefix(MEET_URL).unwrap_or(input);
    let lowercase = input.to_ascii_lowercase();
    if is_meeting_code(&lowercase) {
        Target::Code(lowercase)
    } else {
        Target::Record(format!("{RECORD_PREFIX}{}", record_id(input)))
    }
}

fn words(parts: &[&str]) -> String {
    let parts: Vec<&str> = parts
        .iter()
        .copied()
        .filter(|part| !part.is_empty())
        .collect();
    parts.join(" ")
}

fn span<'a>(start: &'a Option<String>, end: &'a Option<String>) -> [&'a str; 2] {
    [
        start.as_deref().unwrap_or_default(),
        end.as_deref().unwrap_or_default(),
    ]
}

pub fn render_space(out: &mut impl Write, space: &Space) -> Result<()> {
    field(out, "space", &space.name)?;
    field_opt(out, "meeting_code", space.meeting_code.as_deref())?;
    field_opt(out, "meeting_uri", space.meeting_uri.as_deref())?;
    if let Some(config) = &space.config {
        field_opt(out, "access_type", config.access_type.as_deref())?;
        field_opt(
            out,
            "entry_point_access",
            config.entry_point_access.as_deref(),
        )?;
        field_opt(out, "moderation", config.moderation.as_deref())?;
    }
    if let Some(active) = &space.active_conference {
        field_opt(
            out,
            "active_conference",
            active.conference_record.as_deref().map(record_id),
        )?;
    }
    Ok(())
}

pub fn render_code(
    out: &mut impl Write,
    space: &Space,
    records: &[ConferenceRecord],
) -> Result<()> {
    field_opt(out, "id", space.meeting_code.as_deref())?;
    render_space(out, space)?;
    for record in records.iter().rev() {
        let [start, end] = span(&record.start_time, &record.end_time);
        field(
            out,
            "conference",
            &words(&[record_id(&record.name), start, end]),
        )?;
    }
    Ok(())
}

fn who(participant: &Participant) -> (String, &'static str) {
    let named = |person: &Option<Person>| {
        person.as_ref().map(|person| {
            person
                .display_name
                .clone()
                .or_else(|| person.user.clone())
                .unwrap_or_default()
        })
    };
    if let Some(name) = named(&participant.signedin_user) {
        (name, "user")
    } else if let Some(name) = named(&participant.anonymous_user) {
        (name, "anonymous")
    } else {
        (named(&participant.phone_user).unwrap_or_default(), "phone")
    }
}

fn artifact_line(artifact: &Artifact) -> String {
    let (label, id) = match (&artifact.drive_destination, &artifact.docs_destination) {
        (Some(drive), _) => ("drive", drive.file.as_deref()),
        (None, Some(docs)) => ("doc", docs.document.as_deref()),
        (None, None) => ("", None),
    };
    let target = id.map(|id| format!("{label}:{id}")).unwrap_or_default();
    let [start, end] = span(&artifact.start_time, &artifact.end_time);
    words(&[
        artifact.state.as_deref().unwrap_or_default(),
        start,
        end,
        &target,
    ])
}

pub fn render_record(
    out: &mut impl Write,
    record: &ConferenceRecord,
    space: &Space,
    participants: &[Participant],
    recordings: &[Artifact],
    transcripts: &[Artifact],
) -> Result<()> {
    field(out, "id", record_id(&record.name))?;
    field_opt(out, "start", record.start_time.as_deref())?;
    field_opt(out, "end", record.end_time.as_deref())?;
    render_space(out, space)?;
    for participant in participants {
        let (name, kind) = who(participant);
        let [start, end] = span(
            &participant.earliest_start_time,
            &participant.latest_end_time,
        );
        field(
            out,
            "participant",
            &words(&[&name, &format!("({kind})"), start, end]),
        )?;
    }
    for recording in recordings {
        field(out, "recording", &artifact_line(recording))?;
    }
    for transcript in transcripts {
        field(out, "transcript", &artifact_line(transcript))?;
    }
    Ok(())
}

pub async fn get<C: Connector>(api: &Api<C>, id: &str, out: &mut impl Write) -> Result<()> {
    match parse_target(id) {
        Target::Code(code) => {
            let filter = format!("space.meeting_code = \"{code}\"");
            let name = format!("spaces/{code}");
            let (space, records) = tokio::try_join!(
                api.meet.space(&name),
                api.meet.records(Some(&filter), RECENT_RECORDS),
            )?;
            render_code(out, &space, &records)
        }
        Target::Record(name) => {
            let record_and_space = async {
                let record = api.meet.record(&name).await?;
                let space = api
                    .meet
                    .space(record.space.as_deref().unwrap_or_default())
                    .await?;
                anyhow::Ok((record, space))
            };
            let ((record, space), participants, recordings, transcripts) = tokio::try_join!(
                record_and_space,
                api.meet.participants(&name),
                api.meet.recordings(&name),
                api.meet.transcripts(&name),
            )?;
            render_record(
                out,
                &record,
                &space,
                &participants,
                &recordings,
                &transcripts,
            )
        }
    }
}
