use std::net::SocketAddr;
use std::sync::{self, Arc, Mutex};

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
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
      .route("/", get(serve_ui))
      .route("/hit", get(hit::get).post(hit::post))
      .route("/max", get(max::get).post(max::post))
      .route("/reset", post(reset::post))
      .with_state(state)
  }
}

// dummy frontend for users to send requests using a gui...
async fn serve_ui() -> Html<&'static str> {
  Html(r#"
<!DOCTYPE html>
<html>
<head>
  <meta charset="UTF-8">
  <title>Counter API</title>
  <style>
    body { font-family: monospace; max-width: 600px; margin: 40px auto; padding: 20px; }
    .section { margin: 30px 0; padding: 20px; border: 1px solid #ccc; }
    button { padding: 10px 20px; margin: 5px; cursor: pointer; }
    input { padding: 8px; margin: 5px; }
    pre { background: #f4f4f4; padding: 10px; border-radius: 4px; }
    h2 { margin-top: 0; }
  </style>
</head>
<body>
  <h1>Counter API</h1>
  
  <div class="section">
    <h2>GET /hit</h2>
    <p>View current counter state</p>
    <button onclick="getHit()">Get Counter</button>
    <pre id="get-hit-result"></pre>
  </div>

  <div class="section">
    <h2>POST /hit</h2>
    <p>Increment the counter</p>
    <button onclick="postHit()">Increment</button>
    <pre id="post-hit-result"></pre>
  </div>

  <div class="section">
    <h2>GET /max</h2>
    <p>View current maximum</p>
    <button onclick="getMax()">Get Max</button>
    <pre id="get-max-result"></pre>
  </div>

  <div class="section">
    <h2>POST /max</h2>
    <p>Update maximum value</p>
    <input type="number" id="new-max" placeholder="Enter new max" value="5">
    <button onclick="postMax()">Update Max</button>
    <pre id="post-max-result"></pre>
  </div>

  <div class="section">
    <h2>POST /reset</h2>
    <p>Reset counter to zero</p>
    <button onclick="postReset()">Reset Counter</button>
    <pre id="post-reset-result"></pre>
  </div>

  <script>
    async function getHit() {
      const res = await fetch('/hit');
      const data = await res.json();
      document.getElementById('get-hit-result').textContent = JSON.stringify(data, null, 2);
    }

    async function postHit() {
      const res = await fetch('/hit', { method: 'POST' });
      const data = await res.json();
      document.getElementById('post-hit-result').textContent = JSON.stringify(data, null, 2);
    }

    async function getMax() {
      const res = await fetch('/max');
      const data = await res.json();
      document.getElementById('get-max-result').textContent = JSON.stringify(data, null, 2);
    }

    async function postMax() {
      const max = document.getElementById('new-max').value;
      const res = await fetch('/max', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ max: parseInt(max) })
      });
      const data = await res.json();
      document.getElementById('post-max-result').textContent = JSON.stringify(data, null, 2);
    }

    async function postReset() {
      const res = await fetch('/reset', { method: 'POST' });
      const data = await res.json();
      document.getElementById('post-reset-result').textContent = JSON.stringify(data, null, 2);
    }
  </script>
</body>
</html>
  "#)
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
  pub struct GetResponse {
    count: u64,
    max: u64,
    saturated: bool,
  }

  #[derive(Serialize)]
  pub struct PostResponse {
    count: u64,
    saturated: bool,
  }

  pub async fn get(State(state): State<AppState>) -> Result<Json<GetResponse>, Error> {
    let counter = state.inner.counter.lock()?;
    Ok(Json(GetResponse {
      count: counter.count().get(),
      max: counter.max().get(),
      saturated: counter::is_saturated(&counter),
    }))
  }

  pub async fn post(State(state): State<AppState>) -> Result<Json<PostResponse>, Error> {
    let mut guard = state.inner.counter.lock()?;
    *guard = guard.clone().inc();

    let saturated = counter::is_saturated(&guard);
    if saturated {
      info!("counter is saturated at {0}/{0}", guard.max().get());
    }

    Ok(Json(PostResponse {
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
