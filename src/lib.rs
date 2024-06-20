#[cfg(feature = "client")]
mod client;

#[cfg(feature = "client")]
pub use client::{Client, Error};

#[cfg(feature = "provider")]
mod provider;
#[cfg(feature = "provider")]
pub use provider::{serve, Provider};

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DomainFilter {
    pub filters: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Endpoint {
    pub dns_name: String,
    pub targets: String,
    pub record_type: String,
    pub set_identifier: String,
    pub record_ttl: i64,
    pub labels: HashMap<String, String>,
    pub provider_specific: Vec<ProviderSpecificProperty>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSpecificProperty {
    pub name: String,
    pub value: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Changes {
    pub create: Vec<Endpoint>,
    pub update_old: Vec<Endpoint>,
    pub update_new: Vec<Endpoint>,
    pub delete: Vec<Endpoint>,
}
