use super::accounts::{self, Email};
use super::{SCOPES, render};
use crate::google::{DEADLINE, Google, Https, connector, https_builder, with_deadline};
use crate::paths::Paths;
use anyhow::{Context, Result};
use hyper_util::client::legacy::Client;
use std::future::Future;
use std::io::Write;
use std::path::Path;
use std::pin::Pin;
use tempfile::NamedTempFile;
use yup_oauth2::authenticator::Authenticator;
use yup_oauth2::authenticator_delegate::InstalledFlowDelegate;
use yup_oauth2::{
    ApplicationSecret, CustomHyperClientBuilder, InstalledFlowAuthenticator,
    InstalledFlowReturnMethod,
};

struct NotLoggedIn(Option<Email>);

impl InstalledFlowDelegate for NotLoggedIn {
    fn present_user_url<'a>(
        &'a self,
        _url: &'a str,
        _need_code: bool,
    ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send + 'a>> {
        let message = match &self.0 {
            Some(email) => format!(
                "no valid token for {email}: run `gdcli auth login` and choose that account"
            ),
            None => "no valid existing token: run `gdcli auth login`".to_owned(),
        };
        Box::pin(async { Err(message) })
    }
}

pub async fn secret(paths: &Paths) -> Result<ApplicationSecret> {
    let path = paths.client_secret();
    yup_oauth2::read_application_secret(&path)
        .await
        .with_context(|| {
            format!(
                "cannot read client secret {}: run `gdcli auth setup`",
                path.display()
            )
        })
}

pub async fn authenticator(
    secret: ApplicationSecret,
    token_file: &Path,
    account: Option<&Email>,
    login: bool,
    client: Client<Https, String>,
) -> Result<Authenticator<Https>> {
    let method = if login {
        InstalledFlowReturnMethod::HTTPRedirect
    } else {
        InstalledFlowReturnMethod::Interactive
    };
    let builder = InstalledFlowAuthenticator::with_client(
        secret,
        method,
        CustomHyperClientBuilder::from(client),
    )
    .persist_tokens_to_disk(token_file);
    let builder = if login {
        builder.force_account_selection(true)
    } else {
        builder.flow_delegate(Box::new(NotLoggedIn(account.cloned())))
    };
    builder.build().await.with_context(|| {
        format!(
            "cannot load {}: run `gdcli auth login`",
            token_file.display()
        )
    })
}

pub async fn login(paths: &Paths, out: &mut impl Write) -> Result<()> {
    let secret = secret(paths).await?;
    accounts::create(paths)?;
    let _ = with_deadline(Some(DEADLINE), accounts::migrate(paths)).await;
    let mut pending = NamedTempFile::with_prefix_in(".login-", paths.accounts())
        .context("cannot create the login file")?;
    pending
        .write_all(b"[]")
        .context("cannot write the login file")?;
    let client = https_builder().build(connector());
    let auth = authenticator(secret, pending.path(), None, true, client.clone()).await?;
    auth.token(&SCOPES).await.context("login failed")?;
    let google = Google::production(client, auth);
    let stored = with_deadline(Some(DEADLINE), accounts::store(paths, pending, &google)).await?;
    render(&stored, out)
}
