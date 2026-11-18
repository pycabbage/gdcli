use hyper_rustls::{HttpsConnector, HttpsConnectorBuilder};
use hyper_util::client::legacy::{Builder, Client, connect::HttpConnector};
use hyper_util::rt::TokioExecutor;

pub type Https = HttpsConnector<HttpConnector>;

pub fn connector() -> Https {
    HttpsConnectorBuilder::new()
        .with_webpki_roots()
        .https_only()
        .enable_http1()
        .enable_http2()
        .build()
}

pub fn builder() -> Builder {
    Client::builder(TokioExecutor::new())
}
