use std::collections::HashMap;
use std::env;
use std::net::SocketAddr;
use std::sync::{self, Arc, Mutex};

use axum::extract::{Query, State};
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

  #[error("unauthorized access")]
  UnauthorizedAccess,

  #[error("failed to parse env var - {0}")]
  EnvVarFailure(#[from] env::VarError),

  #[error("ui is currently disabled")]
  UiDisabled,
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
async fn serve_ui(
  Query(params): Query<HashMap<String, String>>,
) -> Result<Html<&'static str>, Error> {
  // totally insecure, should probably change, but its not crucial
  let password = env::var("UI_PASSWORD")?;
  
  // bit of a weird case but when UI_PASSWORD isnt defined, docker compose
  // defines it anyway, just empty... so we check that case here...
  if password == "" {
    return Err(Error::UiDisabled);
  }

  if params.get("password") != Some(&password) {
    return Err(Error::UnauthorizedAccess);
  }

  Ok(Html(
    r#"
<!DOCTYPE html>
<html>
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Mighty Counter</title>
  <style>
    * { margin: 0; padding: 0; box-sizing: border-box; }
    body { 
      font-family: -apple-system, BlinkMacSystemFont, 'SF Pro Display', 'Segoe UI', sans-serif;
      display: flex;
      align-items: center;
      justify-content: center;
      min-height: 100vh;
      background: #f5f5f7;
      color: #1d1d1f;
    }
    .container {
      text-align: center;
      padding: 40px;
      max-width: 600px;
    }
    h1 {
      font-size: 2.5rem;
      margin-bottom: 1rem;
      font-weight: 600;
      letter-spacing: -0.5px;
      color: #1d1d1f;
    }
    .subtitle {
      font-size: 1.1rem;
      color: #86868b;
      margin-bottom: 3rem;
      font-weight: 400;
    }
    #queue-button {
      font-size: 6rem;
      font-weight: 700;
      padding: 80px 100px;
      background: white;
      border: 2px solid #e5e5e7;
      border-radius: 16px;
      color: #1d1d1f;
      cursor: pointer;
      transition: all 0.2s ease;
      font-family: 'SF Mono', Monaco, monospace;
      box-shadow: 0 4px 16px rgba(0, 0, 0, 0.08);
      min-width: 420px;
      letter-spacing: -3px;
    }
    #queue-button:hover:not(:disabled) {
      background: #fafafa;
      box-shadow: 0 6px 24px rgba(0, 0, 0, 0.12);
      transform: translateY(-2px);
    }
    #queue-button:active:not(:disabled) {
      transform: translateY(0);
      box-shadow: 0 2px 8px rgba(0, 0, 0, 0.08);
    }
    #queue-button:disabled {
      cursor: not-allowed;
      opacity: 0.4;
      background: #fafafa;
    }
  </style>
</head>
<body>
  <div class="container">
    <h1>Mighty Count</h1>
    <p class="subtitle">click for pod!</p>
    <button id="queue-button" onclick="joinQueue()">
      <span id="count-display">--/--</span>
    </button>
  </div>

  <script>
    let hasClicked = false;

    async function loadCount() {
      try {
        const res = await fetch('/hit');
        const data = await res.json();
        document.getElementById('count-display').textContent = 
          `${data.count}/${data.max}`;
        
        if (data.saturated) {
          document.getElementById('queue-button').disabled = true;
        }
      } catch (err) {
        console.error('Failed to load:', err);
      }
    }

    async function joinQueue() {
      if (hasClicked) return;
      
      hasClicked = true;
      const button = document.getElementById('queue-button');
      button.disabled = true;

      try {
        const res = await fetch('/hit', { method: 'POST' });
        const data = await res.json();
        
        document.getElementById('count-display').textContent = 
          `${data.count}/${data.max || '?'}`;
      } catch (err) {
        hasClicked = false;
        button.disabled = false;
      }
    }

    loadCount();
  </script>
</body>
</html>
  "#,
  ))
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
  pub struct GetResponse {
    max: u64,
  }

  #[derive(Serialize)]
  pub struct PostResponse {
    max: u64,
  }

  pub async fn get(State(state): State<AppState>) -> Result<Json<GetResponse>, Error> {
    let counter = state.inner.counter.lock()?;
    Ok(Json(GetResponse {
      max: counter.max().get(),
    }))
  }

  pub async fn post(
    State(state): State<AppState>,
    Json(req): Json<PostRequest>,
  ) -> Result<Json<PostResponse>, Error> {
    let mut guard = state.inner.counter.lock()?;
    let new_max = Max::new(req.max)?;
    *guard = counter::update_max(guard.clone(), new_max);
    Ok(Json(PostResponse {
      max: guard.max().get(),
    }))
  }
}

mod reset {
  use super::*;

  #[derive(Serialize)]
  pub struct PostResponse {
    count: u64,
    max: u64,
  }

  pub async fn post(State(state): State<AppState>) -> Result<Json<PostResponse>, Error> {
    let mut guard = state.inner.counter.lock()?;
    *guard = counter::reset(guard.clone());
    Ok(Json(PostResponse {
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
  let () = server.run().await?;
  info!("spinning down server...");

  Ok(())
}
