use std::str::FromStr;

use reqwest::{header::CONTENT_TYPE, IntoUrl, Method, Url};
use serde::{de::DeserializeOwned, Serialize};
use tracing::error;

use crate::{Changes, Endpoint};

pub struct Client {
    /// Prefix of the API endpoints.
    ///
    /// If for example your healthz endpoint is at:
    ///
    ///     http://localhost:9998/external-dns/healthz
    ///
    /// Then your domain should be:
    ///
    ///     http://localhost:9998/external-dns
    domain: String,
    client: reqwest::Client,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("reqwest: {0}")]
    Reqwest(#[from] reqwest::Error),
    #[error("serialization: {0}")]
    Serialization(serde_json::Error),
    #[error("deserialization: {0}")]
    Deserialization(serde_json::Error),
}

impl Client {
    /// Prefix of the API endpoints.
    ///
    /// If for example your healthz endpoint is at:
    ///
    ///     http://localhost:9998/external-dns/healthz
    ///
    /// Then your domain should be:
    ///
    ///     http://localhost:9998/external-dns
    pub fn new(domain: &str) -> Self {
        Client {
            domain: domain.to_string(),
            client: reqwest::Client::new(),
        }
    }

    fn url(&self, path: &str) -> Url {
        Url::from_str(&format!("{}/{path}", self.domain)).unwrap()
    }

    async fn request<I, O>(&self, method: Method, url: impl IntoUrl, body: I) -> Result<O, Error>
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
            .get(self.url("healthz"))
            .send()
            .await?
            .text()
            .await?)
    }

    /// Apply the given [`Changes`]
    pub async fn set_records(&self, changes: Changes) -> Result<(), Error> {
        self.request(Method::POST, self.url("setRecords"), changes)
            .await
    }

    /// Get all records.
    pub async fn get_records(&self) -> Result<Vec<Endpoint>, Error> {
        self.request(Method::GET, self.url("getRecords"), ()).await
    }

    /// Adjust endpoints according to the given list of endpoints.
    pub async fn adjust_endpoints(&self, endpoints: Vec<Endpoint>) -> Result<Vec<Endpoint>, Error> {
        self.request(Method::POST, self.url("adjustEndpoints"), endpoints)
            .await
    }
}
