use crate::auth::{self, Token};
use crate::http::{self, Https};
use crate::meet::Meet;
use crate::paths::Paths;
use anyhow::Result;
use google_apis_common::{Client, Connector, GetToken};
use google_calendar3::CalendarHub;
use google_drive3::DriveHub;

pub struct Endpoints {
    pub google: String,
    pub meet: String,
}

impl Default for Endpoints {
    fn default() -> Self {
        Self {
            google: "https://www.googleapis.com/".to_owned(),
            meet: "https://meet.googleapis.com/".to_owned(),
        }
    }
}

pub struct Api<C> {
    pub calendar: CalendarHub<C>,
    pub drive: DriveHub<C>,
    pub meet: Meet<C>,
}

impl<C: Connector> Api<C> {
    pub fn new<A: GetToken + Clone + 'static>(
        client: Client<C>,
        auth: A,
        endpoints: &Endpoints,
    ) -> Self {
        let mut calendar = CalendarHub::new(client.clone(), auth.clone());
        calendar.base_url(format!("{}calendar/v3/", endpoints.google));
        let mut drive = DriveHub::new(client.clone(), auth.clone());
        drive.base_url(format!("{}drive/v3/", endpoints.google));
        let meet = Meet::new(client, auth, format!("{}v2/", endpoints.meet));
        Self {
            calendar,
            drive,
            meet,
        }
    }
}

impl Api<Https> {
    pub async fn connect(paths: &Paths) -> Result<Self> {
        let connector = http::connector();
        let token = Token::new(auth::authenticator(paths, false, connector.clone()).await?);
        Ok(Self::new(
            http::builder().build(connector),
            token,
            &Endpoints::default(),
        ))
    }
}
