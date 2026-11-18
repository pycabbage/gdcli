use super::SCOPES;
use crate::google::{Https, connector, https_builder};
use crate::paths::Paths;
use anyhow::{Context, Result};
use hyper_util::client::legacy::Client;
use std::future::Future;
use std::io::Write;
use std::os::unix::fs::DirBuilderExt;
use std::pin::Pin;
use yup_oauth2::authenticator::Authenticator;
use yup_oauth2::authenticator_delegate::InstalledFlowDelegate;
use yup_oauth2::{CustomHyperClientBuilder, InstalledFlowAuthenticator, InstalledFlowReturnMethod};

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
    client: Client<Https, String>,
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
        CustomHyperClientBuilder::from(client),
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
        .create(&paths.data)
        .with_context(|| format!("cannot create {}", paths.data.display()))?;
    match std::fs::remove_file(paths.token()) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
            return Err(error).context("cannot reset the stored token");
        }
        _ => {}
    }
    let client = https_builder().build(connector());
    let auth = authenticator(paths, true, client).await?;
    auth.token(&SCOPES).await.context("login failed")?;
    writeln!(
        out,
        "logged in; token stored at {}",
        paths.token().display()
    )?;
    Ok(())
}
