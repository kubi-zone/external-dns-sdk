#[cfg(feature = "client")]
mod client;

#[cfg(feature = "client")]
pub use client::{Client, Error};

#[cfg(feature = "provider")]
mod provider;
use kubizone_common::{DomainName, Type};
#[cfg(feature = "provider")]
pub use provider::{serve, Provider};

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct DomainFilter {
    pub filters: Vec<String>,
}

/// Uniquely identifiable parts of an Endpoint.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub struct EndpointIdent {
    pub dns_name: DomainName,
    pub record_type: Type,
    pub set_identifier: String,
}

/// Domain and record type with one or more "targets" (values).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Endpoint {
    /// Uniquely identifiable record name.
    #[serde(flatten)]
    pub identity: EndpointIdent,

    /// One or more "targets", that is the R-data values returned
    /// when this record is queried.
    pub targets: Vec<String>,

    /// Time-To-Live.
    #[serde(rename = "recordTTL")]
    pub record_ttl: i64,

    /// One or more labels associated with the record, if
    /// supported by the underlying provider.
    #[serde(default)]
    pub labels: HashMap<String, String>,

    /// Provider-specific properties associated with the endpoint.
    #[serde(default)]
    pub provider_specific: Vec<ProviderSpecificProperty>,
}

/// Provider-specific properties associated with an [`Endpoint`]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSpecificProperty {
    /// Name of the property.
    pub name: String,

    /// Value of the property.
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Changes {
    pub create: Vec<Endpoint>,
    pub update_old: Vec<Endpoint>,
    pub update_new: Vec<Endpoint>,
    pub delete: Vec<Endpoint>,
}

/// Change to apply to the record set held by the provider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    /// Update the `old` endpoint to match the `new` one.
    Update {
        /// Existing endpoint which should be updated.
        old: Endpoint,
        new: Endpoint,
    },

    /// Delete the contained endpoint.
    Delete(Endpoint),

    /// Create a new endpoint.
    Create(Endpoint),
}

impl From<Changes> for Vec<Change> {
    fn from(changes: Changes) -> Self {
        let mut out = Vec::new();

        for endpoint in changes.delete {
            out.push(Change::Delete(endpoint));
        }

        for old in changes.update_old {
            if let Some(new) = changes
                .update_new
                .iter()
                .find(|new| new.identity == old.identity)
                .cloned()
            {
                out.push(Change::Update { old, new })
            }
        }

        for endpoint in changes.create {
            out.push(Change::Create(endpoint))
        }

        out
    }
}

impl From<Vec<Change>> for Changes {
    fn from(value: Vec<Change>) -> Self {
        let mut out = Changes {
            create: vec![],
            update_old: vec![],
            update_new: vec![],
            delete: vec![],
        };

        for change in value {
            match change {
                Change::Update { old, new } => {
                    out.update_old.push(old);
                    out.update_new.push(new);
                }
                Change::Delete(endpoint) => out.delete.push(endpoint),
                Change::Create(endpoint) => out.create.push(endpoint),
            }
        }

        out
    }
}

/// Utility trait for computing a differential from two lists of endpoints.
pub trait EndpointDiff {
    /// Compare current (self) and desired (other) state, and compute a list of
    /// changes to move from one state to the other.
    ///
    /// * Endpoints contained in `self` but not in `other` will yield a [`Change::Delete`].
    /// * Endpoints contained in `other` but not in `self` will yield a [`Change::Create`].
    /// * Endpoints contained in both `self` and `other` will yield a [`Change::Update`],
    ///     *if* the entries are not identical.
    fn difference(self, other: Self) -> Vec<Change>;
}

impl EndpointDiff for Vec<Endpoint> {
    fn difference(self, other: Self) -> Vec<Change> {
        let old: HashMap<EndpointIdent, Endpoint> = HashMap::from_iter(
            self.into_iter()
                .map(|endpoint| (endpoint.identity.clone(), endpoint)),
        );
        let new: HashMap<EndpointIdent, Endpoint> = HashMap::from_iter(
            other
                .into_iter()
                .map(|endpoint| (endpoint.identity.clone(), endpoint)),
        );

        let old_keys: HashSet<_> = old.keys().collect();
        let new_keys: HashSet<_> = new.keys().collect();

        println!("{old_keys:#?}\n{new_keys:#?}");

        let creates = new_keys
            .difference(&old_keys)
            .filter_map(|identity| new.get(identity))
            .cloned()
            .map(Change::Create);

        let deletes = old_keys
            .difference(&new_keys)
            .filter_map(|identity| old.get(identity))
            .cloned()
            .map(Change::Delete);

        let updates = old_keys.intersection(&new_keys).filter_map(|identity| {
            let old = old.get(identity)?.clone();
            let new = new.get(identity)?.clone();

            if old == new {
                return None;
            }

            Some(Change::Update { old, new })
        });

        deletes.into_iter().chain(updates).chain(creates).collect()
    }
}

#[cfg(test)]
#[test]
fn difference_calculation() {
    let a = vec![
        Endpoint {
            identity: EndpointIdent {
                dns_name: DomainName::try_from("update.org.").unwrap(),
                record_type: Type::A,
                set_identifier: String::new(),
            },
            targets: vec!["192.168.0.1".to_string()],
            record_ttl: 300,
            labels: HashMap::default(),
            provider_specific: Vec::new(),
        },
        Endpoint {
            identity: EndpointIdent {
                dns_name: DomainName::try_from("delete.org.").unwrap(),
                record_type: Type::A,
                set_identifier: String::new(),
            },
            targets: vec!["192.168.0.1".to_string()],
            record_ttl: 300,
            labels: HashMap::default(),
            provider_specific: Vec::new(),
        },
    ];

    let b = vec![
        Endpoint {
            identity: EndpointIdent {
                dns_name: DomainName::try_from("update.org.").unwrap(),
                record_type: Type::A,
                set_identifier: String::new(),
            },
            targets: vec!["192.168.0.2".to_string()],
            record_ttl: 300,
            labels: HashMap::default(),
            provider_specific: Vec::new(),
        },
        Endpoint {
            identity: EndpointIdent {
                dns_name: DomainName::try_from("create.org.").unwrap(),
                record_type: Type::A,
                set_identifier: String::new(),
            },
            targets: vec!["192.168.0.1".to_string()],
            record_ttl: 300,
            labels: HashMap::default(),
            provider_specific: Vec::new(),
        },
    ];

    let changes = a.difference(b);

    assert_eq!(
        changes,
        vec![
            Change::Delete(Endpoint {
                identity: EndpointIdent {
                    dns_name: DomainName::try_from("delete.org.").unwrap(),
                    record_type: Type::A,
                    set_identifier: String::new(),
                },
                targets: vec!["192.168.0.1".to_string()],
                record_ttl: 300,
                labels: HashMap::default(),
                provider_specific: Vec::new(),
            }),
            Change::Update {
                old: Endpoint {
                    identity: EndpointIdent {
                        dns_name: DomainName::try_from("update.org.").unwrap(),
                        record_type: Type::A,
                        set_identifier: String::new(),
                    },
                    targets: vec!["192.168.0.1".to_string()],
                    record_ttl: 300,
                    labels: HashMap::default(),
                    provider_specific: Vec::new(),
                },
                new: Endpoint {
                    identity: EndpointIdent {
                        dns_name: DomainName::try_from("update.org.").unwrap(),
                        record_type: Type::A,
                        set_identifier: String::new(),
                    },
                    targets: vec!["192.168.0.2".to_string()],
                    record_ttl: 300,
                    labels: HashMap::default(),
                    provider_specific: Vec::new(),
                }
            },
            Change::Create(Endpoint {
                identity: EndpointIdent {
                    dns_name: DomainName::try_from("create.org.").unwrap(),
                    record_type: Type::A,
                    set_identifier: String::new(),
                },
                targets: vec!["192.168.0.1".to_string()],
                record_ttl: 300,
                labels: HashMap::default(),
                provider_specific: Vec::new(),
            })
        ]
    )
}
