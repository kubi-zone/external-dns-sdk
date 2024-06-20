use axum::async_trait;
use external_dns_sdk::{Changes, Client, DomainFilter, Endpoint, Provider};
use tracing::info;

struct DebugProvider {}

#[async_trait]
impl Provider for DebugProvider {
    type Error = &'static str;

    async fn init(&self) -> Result<DomainFilter, Self::Error> {
        info!("initializing");

        Ok(DomainFilter { filters: vec![] })
    }

    async fn healthz(&self) -> Result<String, Self::Error> {
        Ok("ok".to_string())
    }

    async fn get_records(&self) -> Result<Vec<Endpoint>, Self::Error> {
        Ok(vec![])
    }

    async fn set_records(&self, _changes: Changes) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn adjust_endpoints(
        &self,
        endpoints: Vec<Endpoint>,
    ) -> Result<Vec<Endpoint>, Self::Error> {
        Ok(endpoints)
    }
}

#[tokio::test]
async fn main() {
    let server =
        tokio::spawn(async move { external_dns_sdk::serve(12333, DebugProvider {}).await });

    let client = Client::new("http://localhost:12333");

    assert_eq!(client.healthz().await.unwrap(), "ok".to_string());
    assert_eq!(client.adjust_endpoints(vec![]).await.unwrap(), vec![]);
    assert_eq!(client.get_records().await.unwrap(), vec![]);

    assert_eq!(
        client
            .set_records(Changes {
                create: vec![],
                update_old: vec![],
                update_new: vec![],
                delete: vec![]
            })
            .await
            .unwrap(),
        ()
    );

    server.abort();
    server.await.ok();
}
