use std::{fmt::Debug, string::FromUtf8Error};

use reqwest::{
    header::{ACCEPT, CONTENT_TYPE},
    Method, Response, StatusCode,
};
use serde::de::DeserializeOwned;
use tracing::{error, instrument, trace};

use crate::{Change, Changes, DomainFilter, Endpoint};

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

    /// Webhook Failure
    #[error("webhook: status code {0}: {1}")]
    Webhook(StatusCode, String),

    /// Response payload is not valid utf8
    #[error("invalid utf8 payload: {0}")]
    InvalidUtf8(#[from] FromUtf8Error),
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

    /// Initialize the webhook service and fetch the domain filter.
    #[instrument(skip(self))]
    pub async fn init(&self) -> Result<Vec<String>, Error> {
        Ok(self
            .client
            .get(self.domain.clone())
            .send()
            .await?
            .json::<DomainFilter>()
            .await?
            .filters)
    }

    /// Check health of the webhook service
    #[instrument(skip(self))]
    pub async fn healthz(&self) -> Result<String, Error> {
        Ok(self
            .client
            .request(Method::GET, self.domain.join("healthz")?)
            .send()
            .await?
            .text()
            .await?)
    }

    /// Apply the given [`Changes`]
    #[instrument(skip(self))]
    pub async fn set_records(&self, changes: Vec<Change>) -> Result<(), Error> {
        let serialized_body =
            serde_json::to_string(&Changes::from(changes)).map_err(Error::Serialization)?;

        let response = self
            .client
            .request(Method::POST, self.domain.join("records")?)
            .body(serialized_body)
            .header(
                CONTENT_TYPE,
                "application/external.dns.webhook+json;version=1",
            )
            .send()
            .await?;

        if response.status().is_success() {
            return Ok(());
        }

        Err(Error::Webhook(
            response.status(),
            String::from_utf8_lossy(&response.bytes().await?).into_owned(),
        ))
    }

    async fn parse_response<T: DeserializeOwned + Debug>(response: Response) -> Result<T, Error> {
        let status = response.status();

        trace!("webhook returned status code: {status}");

        let payload = response
            .bytes()
            .await
            .map_err(|err| {
                error!("failed to extract response payload: {err}");
                err
            })?
            .to_vec();

        let payload = String::from_utf8(payload).map_err(|err| {
            error!("failed to parse payload as valid utf8: {err}");
            err
        })?;

        if status.is_success() {
            let payload = serde_json::from_str::<T>(&payload).map_err(|err| {
                error!("failed to parse json payload: {err} ({payload})");
                Error::Deserialization(err)
            })?;

            trace!("api returned response: {payload:?}");
            Ok(payload)
        } else {
            Err(Error::Webhook(status, payload))
        }
    }

    /// Get all records.
    #[instrument(skip(self))]
    pub async fn get_records(&self) -> Result<Vec<Endpoint>, Error> {
        let response = self
            .client
            .request(Method::GET, self.domain.join("records")?)
            .header(ACCEPT, "application/external.dns.webhook+json;version=1")
            .send()
            .await?;

        Self::parse_response(response).await
    }

    /// Adjust endpoints according to the given list of endpoints.
    #[instrument(skip(self))]
    pub async fn adjust_endpoints(&self, endpoints: Vec<Endpoint>) -> Result<Vec<Endpoint>, Error> {
        let serialized_body = serde_json::to_string(&endpoints).map_err(Error::Serialization)?;

        let response = self
            .client
            .request(Method::POST, self.domain.join("adjustendpoints")?)
            .body(serialized_body)
            .header(
                CONTENT_TYPE,
                "application/external.dns.webhook+json;version=1",
            )
            .header(ACCEPT, "application/external.dns.webhook+json;version=1")
            .send()
            .await?;

        Self::parse_response(response).await
    }
}
