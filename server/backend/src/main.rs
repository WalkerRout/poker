use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};

use serde::{Deserialize, Serialize};

use tracing::{info, instrument};
use tracing_subscriber::filter::LevelFilter;

use lib_service::prelude::*;

#[derive(Clone)]
struct AppState {
  counter: Arc<AtomicU64>,
}

#[derive(Serialize)]
struct CountResponse {
  value: u64,
}

#[derive(Deserialize)]
struct IncRequest {
  by: u64,
}

struct CounterService {
  initial: u64,
}

impl CounterService {
  fn new(initial: u64) -> Self {
    Self { initial }
  }
}

impl Service for CounterService {
  fn router(&self) -> Router {
    let state = AppState {
      counter: Arc::new(AtomicU64::new(self.initial)),
    };

    Router::new()
      .route("/count", get(get_count).post(inc_one))
      .route("/count/by", post(inc_by))
      .with_state(state)
  }
}

async fn get_count(State(state): State<AppState>) -> Json<CountResponse> {
  let v = state.counter.load(Ordering::Relaxed);
  info!("request to get count: {v}");
  Json(CountResponse { value: v })
}

async fn inc_one(State(state): State<AppState>) -> Json<CountResponse> {
  let v = state.counter.fetch_add(1, Ordering::Relaxed) + 1;
  info!(
    "incrementing count by 1: {original_v} + 1 = {v}",
    original_v = v.saturating_sub(1)
  );
  Json(CountResponse { value: v })
}

async fn inc_by(
  State(state): State<AppState>,
  Json(IncRequest { by }): Json<IncRequest>,
) -> Json<CountResponse> {
  let v = state.counter.fetch_add(by, Ordering::Relaxed) + by;
  info!(
    "incrementing count by 1: {v_sub_by} + {by} = {v}",
    v_sub_by = v.saturating_sub(by)
  );
  Json(CountResponse { value: v })
}

#[instrument(name = "COUNTER")]
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), anyhow::Error> {
  tracing_subscriber::fmt()
    .with_max_level(LevelFilter::INFO)
    .with_target(false)
    .with_thread_ids(true)
    .with_ansi(false)
    .init();

  log_panics::init();

  let addr: SocketAddr = "0.0.0.0:3000".parse()?;
  let service = CounterService::new(0);
  let server = Server::new(addr, service).await?;

  info!("spinning up server...");
  server.run().await?;
  info!("spinning down server...");

  Ok(())
}
