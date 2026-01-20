use axum::{
    extract::Query,
    http::StatusCode,
    response::Json,
    routing::get,
    Router,
};
use serde::{Deserialize, Serialize};
use tower_http::cors::CorsLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let app = Router::new()
        .route("/health", get(healthcheck))
        .route("/query", get(query))
        .layer(CorsLayer::permissive());

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    tracing::info!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}

async fn healthcheck() -> StatusCode {
    StatusCode::OK
}

#[derive(Debug, Deserialize)]
struct QueryParams {
    q: Option<String>,
}

#[derive(Debug, Serialize)]
struct QueryResponse {
    query: String,
    results: Vec<String>,
}

async fn query(Query(params): Query<QueryParams>) -> Json<QueryResponse> {
    let query = params.q.unwrap_or_default();
    
    // TODO: implement actual query logic
    let results = vec![
        format!("Result 1 for: {}", query),
        format!("Result 2 for: {}", query),
    ];

    Json(QueryResponse { query, results })
}