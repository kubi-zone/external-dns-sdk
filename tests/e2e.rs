use std::{
    collections::HashMap,
    net::{Ipv4Addr, SocketAddrV4},
    sync::Arc,
};

use axum::async_trait;
use external_dns_sdk::{Change, Client, Endpoint, EndpointDiff, EndpointIdent, Provider};
use kubizone_common::{DomainName, Type};
use tokio::sync::RwLock;
use tracing::{debug, info, info_span, instrument, level_filters::LevelFilter, trace};

struct DebugProvider {
    inner: Arc<RwLock<Vec<Endpoint>>>,
}

impl DebugProvider {
    pub fn new() -> Self {
        DebugProvider {
            inner: Arc::new(RwLock::new(vec![])),
        }
    }

    async fn find_index(&self, needle: &Endpoint) -> Option<usize> {
        self.inner
            .read()
            .await
            .iter()
            .enumerate()
            .find_map(|(index, endpoint)| (endpoint.identity == needle.identity).then_some(index))
    }
}

#[async_trait]
impl Provider for DebugProvider {
    type Error = &'static str;

    #[instrument(skip(self))]
    async fn init(&self) -> Result<Vec<DomainName>, Self::Error> {
        info_span!("init");

        info!("initializing");
        Ok(Vec::new())
    }

    #[instrument(skip(self))]
    async fn healthz(&self) -> Result<String, Self::Error> {
        Ok("ok".to_string())
    }

    #[instrument(skip(self))]
    async fn get_records(&self) -> Result<Vec<Endpoint>, Self::Error> {
        Ok(self.inner.read().await.clone())
    }

    #[instrument(skip(self))]
    async fn set_records(&self, changes: Vec<Change>) -> Result<(), Self::Error> {
        info_span!("set_records");
        for change in changes {
            match change {
                Change::Update { old, new } => {
                    trace!(
                        "updating {} from {} to {}",
                        old.identity.dns_name,
                        old.targets.join(", "),
                        new.targets.join(", ")
                    );

                    if let Some(index) = self.find_index(&old).await {
                        self.inner.write().await[index] = new;
                    }
                }
                Change::Delete(endpoint) => {
                    trace!("deleting {}", endpoint.identity.dns_name,);
                    if let Some(index) = self.find_index(&endpoint).await {
                        self.inner.write().await.remove(index);
                    }
                }
                Change::Create(endpoint) => {
                    trace!("creating {}", endpoint.identity.dns_name,);
                    self.inner.write().await.push(endpoint)
                }
            }
        }
        Ok(())
    }

    #[instrument(skip(self))]
    async fn adjust_endpoints(
        &self,
        endpoints: Vec<Endpoint>,
    ) -> Result<Vec<Endpoint>, Self::Error> {
        let changes = self.inner.read().await.clone().difference(endpoints);
        debug!("{changes:?}");

        self.set_records(changes).await?;
        self.get_records().await
    }
}

#[tokio::test]
async fn main() {
    tracing_subscriber::fmt()
        .with_max_level(LevelFilter::TRACE)
        .init();

    let server = tokio::spawn(async move {
        external_dns_sdk::serve(
            SocketAddrV4::new(Ipv4Addr::LOCALHOST, 12333).into(),
            DebugProvider::new(),
        )
        .await
    });

    let client = Client::new("http://localhost:12333").unwrap();

    assert_eq!(client.healthz().await.unwrap(), "ok".to_string());
    assert_eq!(client.adjust_endpoints(vec![]).await.unwrap(), vec![]);
    assert_eq!(client.get_records().await.unwrap(), vec![]);

    let initial_state = vec![
        Endpoint {
            identity: EndpointIdent {
                dns_name: DomainName::try_from("update.org").unwrap(),
                record_type: Type::A,
                set_identifier: None,
            },
            targets: vec!["192.168.0.1".to_string()],
            record_ttl: Some(300),
            labels: HashMap::default(),
            provider_specific: Vec::new(),
        },
        Endpoint {
            identity: EndpointIdent {
                dns_name: DomainName::try_from("delete.org").unwrap(),
                record_type: Type::A,
                set_identifier: None,
            },
            targets: vec!["192.168.0.1".to_string()],
            record_ttl: Some(300),
            labels: HashMap::default(),
            provider_specific: Vec::new(),
        },
    ];

    client
        .set_records(vec![].difference(initial_state.clone()))
        .await
        .unwrap();

    assert_eq!(client.get_records().await.unwrap(), initial_state);

    let new_state = vec![
        Endpoint {
            identity: EndpointIdent {
                dns_name: DomainName::try_from("update.org").unwrap(),
                record_type: Type::A,
                set_identifier: None,
            },
            targets: vec!["192.168.0.2".to_string()],
            record_ttl: Some(300),
            labels: HashMap::default(),
            provider_specific: Vec::new(),
        },
        Endpoint {
            identity: EndpointIdent {
                dns_name: DomainName::try_from("create.org").unwrap(),
                record_type: Type::A,
                set_identifier: None,
            },
            targets: vec!["192.168.0.1".to_string()],
            record_ttl: Some(300),
            labels: HashMap::default(),
            provider_specific: Vec::new(),
        },
    ];

    client
        .set_records(initial_state.difference(new_state.clone()))
        .await
        .unwrap();

    assert_eq!(client.get_records().await.unwrap(), new_state);

    client
        .set_records(vec![
            Change::Delete(Endpoint {
                identity: EndpointIdent {
                    dns_name: DomainName::try_from("update.org").unwrap(),
                    record_type: Type::A,
                    set_identifier: None,
                },
                targets: vec!["192.168.0.2".to_string()],
                record_ttl: Some(300),
                labels: HashMap::default(),
                provider_specific: Vec::new(),
            }),
            Change::Create(Endpoint {
                identity: EndpointIdent {
                    dns_name: DomainName::try_from("new.org").unwrap(),
                    record_type: Type::A,
                    set_identifier: None,
                },
                targets: vec!["192.168.0.3".to_string()],
                record_ttl: Some(300),
                labels: HashMap::default(),
                provider_specific: Vec::new(),
            }),
        ])
        .await
        .unwrap();

    assert_eq!(
        client.get_records().await.unwrap(),
        vec![
            Endpoint {
                identity: EndpointIdent {
                    dns_name: DomainName::try_from("create.org").unwrap(),
                    record_type: Type::A,
                    set_identifier: None,
                },
                targets: vec!["192.168.0.1".to_string()],
                record_ttl: Some(300),
                labels: HashMap::default(),
                provider_specific: Vec::new(),
            },
            Endpoint {
                identity: EndpointIdent {
                    dns_name: DomainName::try_from("new.org").unwrap(),
                    record_type: Type::A,
                    set_identifier: None,
                },
                targets: vec!["192.168.0.3".to_string()],
                record_ttl: Some(300),
                labels: HashMap::default(),
                provider_specific: Vec::new(),
            }
        ]
    );

    server.abort();
    server.await.ok();
}
