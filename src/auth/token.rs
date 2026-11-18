use super::SCOPES;
use crate::http::{self, Https};
use crate::paths::Paths;
use anyhow::{Context, Result};
use google_apis_common::GetToken;
use std::future::Future;
use std::io::Write;
use std::os::unix::fs::DirBuilderExt;
use std::pin::Pin;
use yup_oauth2::authenticator::Authenticator;
use yup_oauth2::authenticator_delegate::InstalledFlowDelegate;
use yup_oauth2::{CustomHyperClientBuilder, InstalledFlowAuthenticator, InstalledFlowReturnMethod};

#[derive(Clone)]
pub struct Token(Authenticator<Https>);

impl Token {
    pub fn new(authenticator: Authenticator<Https>) -> Self {
        Self(authenticator)
    }
}

impl GetToken for Token {
    fn get_token<'a>(
        &'a self,
        _scopes: &'a [&str],
    ) -> Pin<
        Box<
            dyn Future<Output = Result<Option<String>, Box<dyn std::error::Error + Send + Sync>>>
                + Send
                + 'a,
        >,
    > {
        Box::pin(async move {
            let token = self.0.token(&SCOPES).await?;
            Ok(token.token().map(str::to_owned))
        })
    }
}

struct NotLoggedIn;

impl InstalledFlowDelegate for NotLoggedIn {
    fn present_user_url<'a>(
        &'a self,
        _url: &'a str,
        _need_code: bool,
    ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send + 'a>> {
        Box::pin(async {
            Err("no valid token (missing, expired or revoked): run `gdcli login`".to_owned())
        })
    }
}

pub async fn authenticator(
    paths: &Paths,
    login: bool,
    connector: Https,
) -> Result<Authenticator<Https>> {
    let secret_path = paths.client_secret();
    let secret = yup_oauth2::read_application_secret(&secret_path)
        .await
        .with_context(|| {
            format!(
                "cannot read client secret {}: run `gdcli setup`",
                secret_path.display()
            )
        })?;
    let method = if login {
        InstalledFlowReturnMethod::HTTPRedirect
    } else {
        InstalledFlowReturnMethod::Interactive
    };
    let builder = InstalledFlowAuthenticator::with_client(
        secret,
        method,
        CustomHyperClientBuilder::from(http::builder().build(connector)),
    )
    .persist_tokens_to_disk(paths.token());
    let builder = if login {
        builder.force_account_selection(true)
    } else {
        builder.flow_delegate(Box::new(NotLoggedIn))
    };
    builder
        .build()
        .await
        .with_context(|| format!("cannot load {}: run `gdcli login`", paths.token().display()))
}

pub async fn login(paths: &Paths, out: &mut impl Write) -> Result<()> {
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(paths.data_dir())
        .with_context(|| format!("cannot create {}", paths.data_dir().display()))?;
    match std::fs::remove_file(paths.token()) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
            return Err(error).context("cannot reset the stored token");
        }
        _ => {}
    }
    let auth = authenticator(paths, true, http::connector()).await?;
    auth.token(&SCOPES).await.context("login failed")?;
    writeln!(
        out,
        "logged in; token stored at {}",
        paths.token().display()
    )?;
    Ok(())
}
