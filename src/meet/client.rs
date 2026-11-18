use super::model::{Artifact, ConferenceRecord, Page, Participant, Space};
use anyhow::{Context, Result, anyhow};
use google_apis_common::{Client, Connector, GetToken};
use http::{Request, header::AUTHORIZATION};
use http_body_util::BodyExt;
use serde::de::DeserializeOwned;
use url::Url;

pub struct Meet<C> {
    client: Client<C>,
    auth: Box<dyn GetToken>,
    root: String,
}

impl<C: Connector> Meet<C> {
    pub fn new<A: GetToken + 'static>(client: Client<C>, auth: A, root: String) -> Self {
        Self {
            client,
            auth: Box::new(auth),
            root,
        }
    }

    async fn get<T: DeserializeOwned>(&self, name: &str, query: &[(&str, &str)]) -> Result<T> {
        let mut url = Url::parse(&self.root)?;
        url.path_segments_mut()
            .map_err(|()| anyhow!("invalid meet root url"))?
            .pop_if_empty()
            .extend(name.split('/'));
        url.query_pairs_mut().extend_pairs(query);
        let token = self
            .auth
            .get_token(&[])
            .await
            .map_err(|error| anyhow!("token retrieval failed: {error}"))?;
        let mut request = Request::get(url.as_str());
        if let Some(token) = token {
            request = request.header(AUTHORIZATION, format!("Bearer {token}"));
        }
        let response = self
            .client
            .request(request.body(google_apis_common::to_body::<String>(None))?)
            .await
            .with_context(|| format!("meet request failed: {name}"))?;
        let status = response.status();
        let bytes = response.into_body().collect().await?.to_bytes();
        if !status.is_success() {
            return Err(match serde_json::from_slice(&bytes) {
                Ok(body) => google_apis_common::Error::BadRequest(body).into(),
                Err(_) => anyhow!("meet api {status}: {}", String::from_utf8_lossy(&bytes)),
            });
        }
        serde_json::from_slice(&bytes).with_context(|| format!("unexpected meet response: {name}"))
    }

    pub async fn space(&self, name: &str) -> Result<Space> {
        self.get(
            name,
            &[(
                "fields",
                "name,meetingUri,meetingCode,config(accessType,entryPointAccess,moderation),activeConference",
            )],
        )
        .await
    }

    pub async fn record(&self, name: &str) -> Result<ConferenceRecord> {
        self.get(name, &[("fields", RECORD_FIELDS)]).await
    }

    pub async fn records(
        &self,
        filter: Option<&str>,
        page_size: u32,
    ) -> Result<Vec<ConferenceRecord>> {
        let size = page_size.to_string();
        let fields = format!("conferenceRecords({RECORD_FIELDS})");
        let mut query = vec![("pageSize", size.as_str()), ("fields", fields.as_str())];
        query.extend(filter.map(|filter| ("filter", filter)));
        Ok(self
            .get::<Page<ConferenceRecord>>("conferenceRecords", &query)
            .await?
            .items)
    }

    pub async fn participants(&self, record: &str) -> Result<Vec<Participant>> {
        let path = format!("{record}/participants");
        let query = [
            ("pageSize", "250"),
            (
                "fields",
                "participants(earliestStartTime,latestEndTime,signedinUser,anonymousUser,phoneUser)",
            ),
        ];
        Ok(self.get::<Page<Participant>>(&path, &query).await?.items)
    }

    pub async fn recordings(&self, record: &str) -> Result<Vec<Artifact>> {
        self.artifacts(record, "recordings", "driveDestination/file")
            .await
    }

    pub async fn transcripts(&self, record: &str) -> Result<Vec<Artifact>> {
        self.artifacts(record, "transcripts", "docsDestination/document")
            .await
    }

    async fn artifacts(
        &self,
        record: &str,
        kind: &str,
        destination: &str,
    ) -> Result<Vec<Artifact>> {
        let path = format!("{record}/{kind}");
        let fields = format!("{kind}(state,startTime,endTime,{destination})");
        let query = [("pageSize", "100"), ("fields", fields.as_str())];
        Ok(self.get::<Page<Artifact>>(&path, &query).await?.items)
    }
}

const RECORD_FIELDS: &str = "name,startTime,endTime,space";
