use std::net::SocketAddr;
use std::sync::{self, Arc, Mutex};

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};

use serde::{Deserialize, Serialize};
use serde_json::json;

use tracing::{info, instrument};
use tracing_subscriber::filter::LevelFilter;

use lib_service::prelude::*;

pub mod counter;

use counter::{Counter, Max};

#[derive(thiserror::Error, Debug)]
enum Error {
  #[error("counter operation failed - {0}")]
  CounterFailure(#[from] counter::Error),

  #[error("mutex lock poisoned")]
  LockFailure,
}

impl<T> From<sync::PoisonError<T>> for Error {
  fn from(_: sync::PoisonError<T>) -> Self {
    Self::LockFailure
  }
}

impl IntoResponse for Error {
  fn into_response(self) -> Response {
    let body = Json(json!({
      "error": self.to_string(),
    }));
    (StatusCode::INTERNAL_SERVER_ERROR, body).into_response()
  }
}

struct CounterService {
  counter: Counter,
}

impl CounterService {
  fn new(max: u64) -> Result<Self, Error> {
    Ok(Self {
      counter: Counter::new(Max::new(max)?),
    })
  }
}

impl Service for CounterService {
  fn router(&self) -> Router {
    let state = AppState {
      inner: Arc::new(AppStateInner {
        counter: Mutex::new(self.counter.clone()),
      }),
    };

    Router::new()
      .route("/hit", get(hit::get).post(hit::post))
      .route("/max", get(max::get).post(max::post))
      .route("/reset", post(reset::post))
      .with_state(state)
  }
}

// insides need to be Sync
struct AppStateInner {
  counter: Mutex<Counter>,
}

#[derive(Clone)]
struct AppState {
  inner: Arc<AppStateInner>,
}

mod hit {
  use super::*;

  #[derive(Serialize)]
  pub enum Response {
    Get {
      count: u64,
      max: u64,
      saturated: bool,
    },
    Post {
      count: u64,
      saturated: bool,
    },
  }

  pub async fn get(State(state): State<AppState>) -> Result<Json<Response>, Error> {
    let counter = state.inner.counter.lock()?;
    Ok(Json(Response::Get {
      count: counter.count().get(),
      max: counter.max().get(),
      saturated: counter::is_saturated(&counter),
    }))
  }

  pub async fn post(State(state): State<AppState>) -> Result<Json<Response>, Error> {
    let mut guard = state.inner.counter.lock()?;
    *guard = guard.clone().inc();

    let saturated = counter::is_saturated(&guard);
    if saturated {
      info!("counter is saturated at {0}/{0}", guard.max().get());
    }

    Ok(Json(Response::Post {
      count: guard.count().get(),
      saturated,
    }))
  }
}

mod max {
  use super::*;

  #[derive(Deserialize)]
  pub struct PostRequest {
    max: u64,
  }

  #[derive(Serialize)]
  pub struct Response {
    max: u64,
  }

  pub async fn get(State(state): State<AppState>) -> Result<Json<Response>, Error> {
    let counter = state.inner.counter.lock()?;
    Ok(Json(Response {
      max: counter.max().get(),
    }))
  }

  pub async fn post(
    State(state): State<AppState>,
    Json(req): Json<PostRequest>,
  ) -> Result<Json<Response>, Error> {
    let mut guard = state.inner.counter.lock()?;
    let new_max = Max::new(req.max)?;
    *guard = counter::update_max(guard.clone(), new_max);
    Ok(Json(Response {
      max: guard.max().get(),
    }))
  }
}

mod reset {
  use super::*;

  #[derive(Serialize)]
  pub struct Response {
    count: u64,
    max: u64,
  }

  pub async fn post(State(state): State<AppState>) -> Result<Json<Response>, Error> {
    let mut guard = state.inner.counter.lock()?;
    *guard = counter::reset(guard.clone());
    Ok(Json(Response {
      count: guard.count().get(),
      max: guard.max().get(),
    }))
  }
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
  let default_max: u64 = 3;
  let service = CounterService::new(default_max)?;
  let server = Server::new(addr, service).await?;

  info!("spinning up server...");
  server.run().await?;
  info!("spinning down server...");

  Ok(())
}
