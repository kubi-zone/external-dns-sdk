use std::{fmt::Display, net::SocketAddr, sync::Arc};

use async_trait::async_trait;
use axum::{
    extract::State,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use kubizone_common::DomainName;
use tokio::net::TcpListener;
use tracing::{info_span, warn};

use crate::{Change, Changes, Endpoint};

/// Utility trait for implementing an external-dns webhook provider.
///
/// By implementing this trait for your type, you can simply construct
/// it and pass it to [`serve`], and you're set.
///
/// See an example in-memory imlpementation in the `e2e.rs` test example.
#[async_trait]
pub trait Provider {
    type Error: Display;
    /// Initialisation and negotiates headers and returns domain filter.
    async fn init(&self) -> Result<Vec<DomainName>, Self::Error>;

    /// Health check
    ///
    /// Used by Kubernetes to make sure service is working.
    ///
    /// Should return "ok".
    async fn healthz(&self) -> Result<String, Self::Error>;

    /// Returns the current records.
    async fn get_records(&self) -> Result<Vec<Endpoint>, Self::Error>;

    /// Apply the given changes.
    async fn set_records(&self, changes: Vec<Change>) -> Result<(), Self::Error>;

    /// Instruct the webhook to adjust the records according to the provided list of endpoints.
    async fn adjust_endpoints(
        &self,
        endpoints: Vec<Endpoint>,
    ) -> Result<Vec<Endpoint>, Self::Error>;
}

struct Context<P: Provider>
where
    Arc<P>:,
{
    provider: Arc<P>,
}

impl<P: Provider> Clone for Context<P> {
    fn clone(&self) -> Self {
        Self {
            provider: self.provider.clone(),
        }
    }
}

/// Run an External-DNS compatible webhook provider, using an Axum server.
pub async fn serve<P: Provider + Send + Sync + 'static>(addr: SocketAddr, provider: P) {
    info_span!("external-dns-sdk");
    let app = Router::new()
        .route("/healthz", get(healthz::<P>))
        .route("/getRecords", get(get_records::<P>))
        .route("/setRecords", post(set_records::<P>))
        .route("/adjustEndpoints", post(adjust_endpoints::<P>))
        .with_state(Context {
            provider: Arc::new(provider),
        });

    let listener = TcpListener::bind(addr).await.unwrap();

    axum::serve(listener, app.into_make_service())
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap();
}

async fn healthz<P: Provider>(State(context): State<Context<P>>) -> impl IntoResponse {
    match context.provider.healthz().await {
        Ok(result) => (axum::http::StatusCode::OK, result),
        Err(err) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            err.to_string(),
        ),
    }
}

async fn get_records<P: Provider>(State(context): State<Context<P>>) -> Response {
    match context.provider.get_records().await {
        Ok(result) => (axum::http::StatusCode::OK, Json(result)).into_response(),
        Err(err) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            err.to_string(),
        )
            .into_response(),
    }
}

async fn set_records<P: Provider>(
    State(context): State<Context<P>>,
    Json(changes): Json<Changes>,
) -> Response {
    let changes = Vec::<Change>::from(changes);

    match context.provider.set_records(changes).await {
        Ok(result) => (axum::http::StatusCode::OK, Json(result)).into_response(),
        Err(err) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            err.to_string(),
        )
            .into_response(),
    }
}

async fn adjust_endpoints<P: Provider>(
    State(context): State<Context<P>>,
    Json(endpoints): Json<Vec<Endpoint>>,
) -> Response {
    match context.provider.adjust_endpoints(endpoints).await {
        Ok(result) => (axum::http::StatusCode::OK, Json(result)).into_response(),
        Err(err) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            err.to_string(),
        )
            .into_response(),
    }
}

async fn shutdown_signal() {
    // Triggers in case of CTRL+C signals
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    // Triggers in case of incoming SIGTERM signal
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    // Await either occurring.
    tokio::select! {
        _ = ctrl_c => {
            warn!("Received CTRl+C signal, initiating graceful shutdown.");
        },
        _ = terminate => {
            warn!("Received SIGTERM signal, initiating graceful shutdown.");
        },
    }
}
