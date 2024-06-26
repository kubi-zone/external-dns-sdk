use reqwest::{header::CONTENT_TYPE, Method};
use serde::{de::DeserializeOwned, Serialize};
use tracing::{error, instrument};

use crate::{Change, Changes, Endpoint};

pub use url::Url;

/// External-DNS Webhook Client.
///
/// Used for interacting with HTTP apis implementing the External-DNS Webhook API.
pub struct Client {
    /// Prefix of the API endpoints.
    ///
    /// If for example your healthz endpoint is at:
    ///
    /// > http://localhost:9998/external-dns/healthz
    ///
    /// Then your domain should be:
    ///
    /// > http://localhost:9998/external-dns
    domain: Url,
    client: reqwest::Client,
}

/// External-DNS Webhook API Error.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Failure during transport.
    #[error("reqwest: {0}")]
    Reqwest(#[from] reqwest::Error),

    /// Failed while serializing request body.
    #[error("serialization: {0}")]
    Serialization(serde_json::Error),

    /// Failed while deserializing response body.
    #[error("deserialization: {0}")]
    Deserialization(serde_json::Error),

    /// Failed while parsing Url.
    #[error("url: {0}")]
    Url(#[from] url::ParseError),
}

impl Client {
    /// Domain is the prefix of the API endpoints.
    ///
    /// If for example your healthz endpoint is at:
    ///
    /// > http://localhost:9998/external-dns/healthz
    ///
    /// Then your domain should be:
    ///
    /// > http://localhost:9998/external-dns
    pub fn new<S: AsRef<str>>(domain: S) -> Result<Self, url::ParseError> {
        Ok(Client {
            domain: Url::parse(domain.as_ref())?,
            client: reqwest::Client::new(),
        })
    }

    #[instrument(skip(self, body))]
    async fn request<I, O>(&self, method: Method, url: Url, body: I) -> Result<O, Error>
    where
        I: Serialize,
        O: DeserializeOwned,
    {
        let serialized_body = serde_json::to_string(&body).map_err(Error::Serialization)?;

        let mut request = self.client.request(method, url);

        // Providing () as the body will yield 'null' after serialization.
        if serialized_body != "null" {
            request = request.body(serialized_body).header(
                CONTENT_TYPE,
                "application/external.dns.webhook+json;version=1",
            );
        }

        let result = request.send().await?.text().await?;

        match serde_json::from_str::<O>(&result) {
            Ok(result) => Ok(result),
            Err(err) => {
                error!("failed to deserialize api result: {err}, {result}");
                Err(Error::Deserialization(err))
            }
        }
    }

    /// Check health of the webhook service
    pub async fn healthz(&self) -> Result<String, Error> {
        Ok(self
            .client
            .get(self.domain.join("healthz")?)
            .send()
            .await?
            .text()
            .await?)
    }

    /// Apply the given [`Changes`]
    pub async fn set_records(&self, changes: Vec<Change>) -> Result<(), Error> {
        self.request(
            Method::POST,
            self.domain.join("records")?,
            Changes::from(changes),
        )
        .await
    }

    /// Get all records.
    pub async fn get_records(&self) -> Result<Vec<Endpoint>, Error> {
        self.request(Method::GET, self.domain.join("records")?, ())
            .await
    }

    /// Adjust endpoints according to the given list of endpoints.
    pub async fn adjust_endpoints(&self, endpoints: Vec<Endpoint>) -> Result<Vec<Endpoint>, Error> {
        self.request(
            Method::POST,
            self.domain.join("adjustendpoints")?,
            endpoints,
        )
        .await
    }
}
