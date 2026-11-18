use crate::api::Api;
use anyhow::Result;
use chrono::{DateTime, Duration, NaiveTime, SecondsFormat, Utc};
use google_apis_common::Connector;
use google_calendar3::api::{Event, EventDateTime};
use std::collections::VecDeque;

pub const WINDOWS_DAYS: [i64; 4] = [7, 30, 180, 730];
pub const PAST_PAGE_SIZE: i32 = 250;
pub const LIST_FIELDS: &str = "nextPageToken,items(id,status,summary,start,end)";

pub fn is_cancelled(event: &Event) -> bool {
    event.status.as_deref() == Some("cancelled")
}

pub fn instant(moment: &EventDateTime) -> Option<DateTime<Utc>> {
    moment.date_time.or_else(|| {
        moment
            .date
            .map(|date| date.and_time(NaiveTime::MIN).and_utc())
    })
}

pub fn render_time(moment: Option<&EventDateTime>) -> String {
    moment.map_or_else(String::new, |moment| {
        match (moment.date_time, moment.date) {
            (Some(date_time), _) => date_time.to_rfc3339_opts(SecondsFormat::Secs, true),
            (None, Some(date)) => date.to_string(),
            (None, None) => String::new(),
        }
    })
}

pub fn has_ended(event: &Event, now: DateTime<Utc>) -> bool {
    event
        .end
        .as_ref()
        .and_then(instant)
        .is_some_and(|end| end <= now)
}

async fn pages<C: Connector>(
    api: &Api<C>,
    fields: &str,
    page_size: i32,
    time_min: DateTime<Utc>,
    time_max: Option<DateTime<Utc>>,
    mut take: impl FnMut(Event) -> bool,
) -> Result<()> {
    let mut token: Option<String> = None;
    loop {
        let mut call = api
            .calendar
            .events()
            .list("primary")
            .single_events(true)
            .order_by("startTime")
            .max_results(page_size)
            .param("fields", fields)
            .time_min(time_min);
        if let Some(time_max) = time_max {
            call = call.time_max(time_max);
        }
        if let Some(token) = &token {
            call = call.page_token(token);
        }
        let (_, page) = call.doit().await?;
        for event in page.items.into_iter().flatten() {
            if !take(event) {
                return Ok(());
            }
        }
        token = page.next_page_token;
        if token.is_none() {
            return Ok(());
        }
    }
}

pub async fn upcoming<C: Connector>(
    api: &Api<C>,
    now: DateTime<Utc>,
    wanted: usize,
    page_size: i32,
    fields: &str,
    keep: impl Fn(&Event) -> bool,
) -> Result<Vec<Event>> {
    let mut found = Vec::with_capacity(wanted);
    pages(api, fields, page_size, now, None, |event| {
        if !is_cancelled(&event) && !has_ended(&event, now) && keep(&event) {
            found.push(event);
        }
        found.len() < wanted
    })
    .await?;
    Ok(found)
}

pub async fn past<C: Connector>(
    api: &Api<C>,
    now: DateTime<Utc>,
    wanted: usize,
    fields: &str,
    keep: impl Fn(&Event) -> bool,
) -> Result<Vec<Event>> {
    let mut tail = VecDeque::with_capacity(wanted + 1);
    for days in WINDOWS_DAYS {
        tail.clear();
        let from = now - Duration::days(days);
        pages(api, fields, PAST_PAGE_SIZE, from, Some(now), |event| {
            if !is_cancelled(&event) && has_ended(&event, now) && keep(&event) {
                if tail.len() == wanted {
                    tail.pop_front();
                }
                tail.push_back(event);
            }
            true
        })
        .await?;
        if tail.len() == wanted {
            break;
        }
    }
    Ok(tail.into())
}
