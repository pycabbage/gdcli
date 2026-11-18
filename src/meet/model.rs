use serde::Deserialize;

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Page<T> {
    #[serde(
        alias = "conferenceRecords",
        alias = "participants",
        alias = "recordings",
        alias = "transcripts"
    )]
    pub items: Vec<T>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ConferenceRecord {
    pub name: String,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    pub space: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Space {
    pub name: String,
    pub meeting_uri: Option<String>,
    pub meeting_code: Option<String>,
    pub config: Option<SpaceConfig>,
    pub active_conference: Option<ActiveConference>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SpaceConfig {
    pub access_type: Option<String>,
    pub entry_point_access: Option<String>,
    pub moderation: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ActiveConference {
    pub conference_record: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Participant {
    pub earliest_start_time: Option<String>,
    pub latest_end_time: Option<String>,
    pub signedin_user: Option<Person>,
    pub anonymous_user: Option<Person>,
    pub phone_user: Option<Person>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Person {
    pub display_name: Option<String>,
    pub user: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Artifact {
    pub state: Option<String>,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    pub drive_destination: Option<Destination>,
    pub docs_destination: Option<Destination>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Destination {
    pub file: Option<String>,
    pub document: Option<String>,
}
