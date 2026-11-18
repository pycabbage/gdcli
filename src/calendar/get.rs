use super::events::render_time;
use crate::api::Api;
use crate::output::{field, field_opt};
use anyhow::Result;
use google_apis_common::Connector;
use google_calendar3::api::Event;
use std::io::Write;

pub const GET_FIELDS: &str = "id,status,summary,description,location,start,end,htmlLink,hangoutLink,eventType,recurringEventId,recurrence,visibility,created,updated,organizer(email,displayName),creator(email,displayName),attendees(email,displayName,responseStatus,optional),conferenceData(conferenceId,conferenceSolution(key(type)),entryPoints(entryPointType,uri))";

pub fn render(out: &mut impl Write, event: &Event) -> Result<()> {
    field_opt(out, "id", event.id.as_deref())?;
    field_opt(out, "status", event.status.as_deref())?;
    field_opt(out, "summary", event.summary.as_deref())?;
    field(out, "start", &render_time(event.start.as_ref()))?;
    field(out, "end", &render_time(event.end.as_ref()))?;
    field_opt(
        out,
        "time_zone",
        event
            .start
            .as_ref()
            .and_then(|start| start.time_zone.as_deref()),
    )?;
    field_opt(out, "location", event.location.as_deref())?;
    field_opt(out, "event_type", event.event_type.as_deref())?;
    field_opt(
        out,
        "recurring_event_id",
        event.recurring_event_id.as_deref(),
    )?;
    for rule in event.recurrence.iter().flatten() {
        field(out, "recurrence", rule)?;
    }
    field_opt(out, "visibility", event.visibility.as_deref())?;
    if let Some(organizer) = &event.organizer {
        field(
            out,
            "organizer",
            &person(&organizer.display_name, &organizer.email),
        )?;
    }
    if let Some(creator) = &event.creator {
        field(
            out,
            "creator",
            &person(&creator.display_name, &creator.email),
        )?;
    }
    for attendee in event.attendees.iter().flatten() {
        let mut line = person(&attendee.display_name, &attendee.email);
        if let Some(status) = &attendee.response_status {
            line = format!("{line} [{status}]");
        }
        field(out, "attendee", &line)?;
    }
    field_opt(out, "html_link", event.html_link.as_deref())?;
    field_opt(out, "meet_link", event.hangout_link.as_deref())?;
    if let Some(conference) = &event.conference_data {
        field_opt(out, "conference_id", conference.conference_id.as_deref())?;
        for entry in conference.entry_points.iter().flatten() {
            field(
                out,
                &format!(
                    "entry_point_{}",
                    entry.entry_point_type.as_deref().unwrap_or("other")
                ),
                entry.uri.as_deref().unwrap_or_default(),
            )?;
        }
    }
    field(out, "created", &stamp(event.created))?;
    field(out, "updated", &stamp(event.updated))?;
    field_opt(out, "description", event.description.as_deref())?;
    Ok(())
}

fn person(name: &Option<String>, email: &Option<String>) -> String {
    match (name.as_deref(), email.as_deref()) {
        (Some(name), Some(email)) => format!("{name} <{email}>"),
        (Some(value), None) | (None, Some(value)) => value.to_owned(),
        (None, None) => String::new(),
    }
}

fn stamp(value: Option<chrono::DateTime<chrono::Utc>>) -> String {
    value.map_or_else(String::new, |value| {
        value.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
    })
}

pub async fn get<C: Connector>(api: &Api<C>, id: &str, out: &mut impl Write) -> Result<()> {
    let (_, event) = api
        .calendar
        .events()
        .get("primary", id)
        .param("fields", GET_FIELDS)
        .doit()
        .await?;
    render(out, &event)
}
