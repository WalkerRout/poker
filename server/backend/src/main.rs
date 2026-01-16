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

#[derive(thiserror::Error, Debug)]
enum Error {
  #[error("mutex lock poisoned")]
  LockFailure,

  #[error("unauthorized access")]
  UnauthorizedAccess,

  #[error("failed to parse env var - {0}")]
  EnvVarFailure(#[from] env::VarError),

  #[error("ui is currently disabled")]
  UiDisabled,

  #[error("invalid winner position {0} (must be 1, 2, or 3)")]
  InvalidWinnerPosition(u8),

  #[error("money values must be non-negative")]
  InvalidMoney,
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
    (StatusCode::BAD_REQUEST, body).into_response()
  }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WinnerSample {
  name: String,
  // unix seconds
  timestamp: i64,
  // 1, 2, or 3
  position: u8,
  // cents
  winnings_cents: i64,
  // cents
  pot_cents: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlayerSample {
  name: String,
  // unix seconds
  timestamp: i64,
  // cents
  buy_in_cents: i64,
  // cents
  pot_cents: i64,
}

// insides need to be Sync
struct AppStateInner {
  winners: Mutex<Vec<WinnerSample>>,
  players: Mutex<Vec<PlayerSample>>,
}

#[derive(Clone)]
struct AppState {
  inner: Arc<AppStateInner>,
}

struct PokerService;

impl PokerService {
  fn new() -> Self {
    Self
  }
}

impl Service for PokerService {
  fn router(&self) -> Router {
    let state = AppState {
      inner: Arc::new(AppStateInner {
        winners: Mutex::new(Vec::new()),
        players: Mutex::new(Vec::new()),
      }),
    };

    Router::new()
      .route("/", get(serve_ui))
      .route("/winners", get(winners::get).post(winners::post))
      .route("/players", get(players::get).post(players::post))
      .with_state(state)
  }
}

// dummy frontend for users to send requests using a gui...
async fn serve_ui(Query(params): Query<HashMap<String, String>>) -> Result<Html<&'static str>, Error> {
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
  <title>Poker Tracker</title>
  <style>
    * { margin: 0; padding: 0; box-sizing: border-box; }
    body {
      font-family: -apple-system, BlinkMacSystemFont, 'SF Pro Display', 'Segoe UI', sans-serif;
      min-height: 100vh;
      background: #f5f5f7;
      color: #1d1d1f;
      padding: 24px;
    }
    .wrap {
      max-width: 1100px;
      margin: 0 auto;
      display: grid;
      gap: 18px;
    }
    header {
      background: white;
      border: 1px solid #e5e5e7;
      border-radius: 14px;
      padding: 18px 18px;
      box-shadow: 0 2px 10px rgba(0,0,0,0.05);
      display: flex;
      align-items: baseline;
      justify-content: space-between;
      gap: 12px;
    }
    header h1 {
      font-size: 1.4rem;
      font-weight: 650;
      letter-spacing: -0.2px;
    }
    header .hint {
      font-size: 0.95rem;
      color: #86868b;
      text-align: right;
    }

    .grid {
      display: grid;
      grid-template-columns: 1fr;
      gap: 18px;
    }
    @media (min-width: 980px) {
      .grid { grid-template-columns: 1fr 1fr; }
    }

    .card {
      background: white;
      border: 1px solid #e5e5e7;
      border-radius: 14px;
      padding: 16px;
      box-shadow: 0 2px 10px rgba(0,0,0,0.05);
      overflow: hidden;
    }
    .card h2 {
      font-size: 1.05rem;
      font-weight: 650;
      margin-bottom: 10px;
      letter-spacing: -0.2px;
    }
    .muted { color: #86868b; font-size: 0.92rem; margin-bottom: 12px; }

    form {
      display: grid;
      gap: 10px;
      margin-bottom: 14px;
    }
    .row {
      display: grid;
      grid-template-columns: 1fr;
      gap: 10px;
    }
    @media (min-width: 600px) {
      .row { grid-template-columns: 1fr 1fr; }
      .row.three { grid-template-columns: 1fr 1fr 1fr; }
    }

    label {
      display: grid;
      gap: 6px;
      font-size: 0.9rem;
      color: #1d1d1f;
      font-weight: 520;
    }
    input, select {
      border: 1px solid #e5e5e7;
      border-radius: 10px;
      padding: 10px 10px;
      font-size: 0.95rem;
      outline: none;
      background: #fff;
    }
    input:focus, select:focus {
      border-color: #c7c7cc;
      box-shadow: 0 0 0 4px rgba(0,0,0,0.05);
    }
    button {
      border: 1px solid #e5e5e7;
      border-radius: 10px;
      padding: 10px 12px;
      font-weight: 650;
      background: #1d1d1f;
      color: white;
      cursor: pointer;
      transition: transform 0.05s ease, opacity 0.2s ease;
    }
    button:active { transform: translateY(1px); }

    .err {
      color: #b42318;
      background: #fffbfa;
      border: 1px solid #fee4e2;
      border-radius: 12px;
      padding: 10px 12px;
      font-size: 0.92rem;
      display: none;
      white-space: pre-wrap;
    }

    table {
      width: 100%;
      border-collapse: collapse;
      font-size: 0.92rem;
    }
    th, td {
      border-top: 1px solid #e5e5e7;
      padding: 10px 8px;
      text-align: left;
      vertical-align: top;
    }
    th {
      color: #86868b;
      font-weight: 650;
      font-size: 0.84rem;
      letter-spacing: 0.2px;
      text-transform: uppercase;
    }
    .right { text-align: right; }
    .nowrap { white-space: nowrap; }
    .pill {
      display: inline-block;
      padding: 3px 8px;
      border-radius: 999px;
      border: 1px solid #e5e5e7;
      background: #fafafa;
      font-size: 0.82rem;
      color: #1d1d1f;
      font-weight: 650;
    }
  </style>
</head>
<body>
  <div class="wrap">
    <header>
      <h1>Poker Tracker</h1>
      <div class="hint">Records top-3 winners + non-winners per game.</div>
    </header>

    <div class="grid">
      <section class="card">
        <h2>Winner (Top 3)</h2>
        <div class="muted">Stores: name, timestamp, position (1–3), winnings, total pot.</div>

        <div id="winner-err" class="err"></div>

        <form id="winner-form" onsubmit="submitWinner(event)">
          <div class="row">
            <label>
              Name
              <input id="winner-name" required placeholder="e.g. Alex" />
            </label>
            <label>
              Game time
              <input id="winner-time" type="datetime-local" required />
            </label>
          </div>

          <div class="row three">
            <label>
              Position
              <select id="winner-pos" required>
                <option value="1">1st</option>
                <option value="2">2nd</option>
                <option value="3">3rd</option>
              </select>
            </label>
            <label>
              Winnings ($)
              <input id="winner-win" type="number" step="0.01" min="0" required placeholder="0.00" />
            </label>
            <label>
              Total pot ($)
              <input id="winner-pot" type="number" step="0.01" min="0" required placeholder="0.00" />
            </label>
          </div>

          <button type="submit">Add winner</button>
        </form>

        <table>
          <thead>
            <tr>
              <th class="nowrap">When</th>
              <th>Name</th>
              <th class="nowrap">Place</th>
              <th class="right nowrap">Winnings</th>
              <th class="right nowrap">Pot</th>
            </tr>
          </thead>
          <tbody id="winners-body">
            <tr><td colspan="5" class="muted">Loading…</td></tr>
          </tbody>
        </table>
      </section>

      <section class="card">
        <h2>Non-winner</h2>
        <div class="muted">Stores: name, timestamp, total buy-in, total pot.</div>

        <div id="player-err" class="err"></div>

        <form id="player-form" onsubmit="submitPlayer(event)">
          <div class="row">
            <label>
              Name
              <input id="player-name" required placeholder="e.g. Sam" />
            </label>
            <label>
              Game time
              <input id="player-time" type="datetime-local" required />
            </label>
          </div>

          <div class="row">
            <label>
              Total buy-in ($)
              <input id="player-buyin" type="number" step="0.01" min="0" required placeholder="0.00" />
            </label>
            <label>
              Total pot ($)
              <input id="player-pot" type="number" step="0.01" min="0" required placeholder="0.00" />
            </label>
          </div>

          <button type="submit">Add non-winner</button>
        </form>

        <table>
          <thead>
            <tr>
              <th class="nowrap">When</th>
              <th>Name</th>
              <th class="right nowrap">Buy-in</th>
              <th class="right nowrap">Pot</th>
            </tr>
          </thead>
          <tbody id="players-body">
            <tr><td colspan="4" class="muted">Loading…</td></tr>
          </tbody>
        </table>
      </section>
    </div>
  </div>

  <script>
    function centsFromDollars(v) {
      const n = Number(v);
      if (!Number.isFinite(n)) return 0;
      return Math.round(n * 100);
    }

    function dollarsFromCents(c) {
      const n = Number(c);
      if (!Number.isFinite(n)) return "0.00";
      return (n / 100).toFixed(2);
    }

    function unixSecondsFromDatetimeLocal(v) {
      // datetime-local has no timezone; browser interprets as local time.
      const ms = Date.parse(v);
      return Math.floor(ms / 1000);
    }

    function fmtWhen(ts) {
      const d = new Date(ts * 1000);
      // compact local display
      return d.toLocaleString(undefined, { year: "numeric", month: "short", day: "2-digit", hour: "2-digit", minute: "2-digit" });
    }

    function showErr(id, msg) {
      const el = document.getElementById(id);
      el.textContent = msg;
      el.style.display = "block";
    }

    function clearErr(id) {
      const el = document.getElementById(id);
      el.textContent = "";
      el.style.display = "none";
    }

    async function loadWinners() {
      const body = document.getElementById("winners-body");
      try {
        const res = await fetch("/winners");
        const data = await res.json();
        body.innerHTML = "";
        if (!Array.isArray(data) || data.length === 0) {
          body.innerHTML = `<tr><td colspan="5" class="muted">No winners recorded.</td></tr>`;
          return;
        }
        for (const w of data) {
          const tr = document.createElement("tr");
          tr.innerHTML = `
            <td class="nowrap">${fmtWhen(w.timestamp)}</td>
            <td>${escapeHtml(w.name)}</td>
            <td class="nowrap"><span class="pill">${w.position}</span></td>
            <td class="right nowrap">$${dollarsFromCents(w.winnings_cents)}</td>
            <td class="right nowrap">$${dollarsFromCents(w.pot_cents)}</td>
          `;
          body.appendChild(tr);
        }
      } catch (err) {
        body.innerHTML = `<tr><td colspan="5" class="muted">Failed to load.</td></tr>`;
      }
    }

    async function loadPlayers() {
      const body = document.getElementById("players-body");
      try {
        const res = await fetch("/players");
        const data = await res.json();
        body.innerHTML = "";
        if (!Array.isArray(data) || data.length === 0) {
          body.innerHTML = `<tr><td colspan="4" class="muted">No non-winners recorded.</td></tr>`;
          return;
        }
        for (const p of data) {
          const tr = document.createElement("tr");
          tr.innerHTML = `
            <td class="nowrap">${fmtWhen(p.timestamp)}</td>
            <td>${escapeHtml(p.name)}</td>
            <td class="right nowrap">$${dollarsFromCents(p.buy_in_cents)}</td>
            <td class="right nowrap">$${dollarsFromCents(p.pot_cents)}</td>
          `;
          body.appendChild(tr);
        }
      } catch (err) {
        body.innerHTML = `<tr><td colspan="4" class="muted">Failed to load.</td></tr>`;
      }
    }

    function escapeHtml(s) {
      return String(s)
        .replaceAll("&", "&amp;")
        .replaceAll("<", "&lt;")
        .replaceAll(">", "&gt;")
        .replaceAll('"', "&quot;")
        .replaceAll("'", "&#039;");
    }

    async function submitWinner(ev) {
      ev.preventDefault();
      clearErr("winner-err");

      const name = document.getElementById("winner-name").value.trim();
      const time = document.getElementById("winner-time").value;
      const pos = Number(document.getElementById("winner-pos").value);
      const winnings = document.getElementById("winner-win").value;
      const pot = document.getElementById("winner-pot").value;

      const payload = {
        name,
        timestamp: unixSecondsFromDatetimeLocal(time),
        position: pos,
        winnings_cents: centsFromDollars(winnings),
        pot_cents: centsFromDollars(pot),
      };

      try {
        const res = await fetch("/winners", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify(payload),
        });
        if (!res.ok) {
          const err = await res.json().catch(() => ({}));
          showErr("winner-err", err.error || "Failed to add winner.");
          return;
        }
        document.getElementById("winner-form").reset();
        await loadWinners();
      } catch (err) {
        showErr("winner-err", "Failed to add winner.");
      }
    }

    async function submitPlayer(ev) {
      ev.preventDefault();
      clearErr("player-err");

      const name = document.getElementById("player-name").value.trim();
      const time = document.getElementById("player-time").value;
      const buyin = document.getElementById("player-buyin").value;
      const pot = document.getElementById("player-pot").value;

      const payload = {
        name,
        timestamp: unixSecondsFromDatetimeLocal(time),
        buy_in_cents: centsFromDollars(buyin),
        pot_cents: centsFromDollars(pot),
      };

      try {
        const res = await fetch("/players", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify(payload),
        });
        if (!res.ok) {
          const err = await res.json().catch(() => ({}));
          showErr("player-err", err.error || "Failed to add non-winner.");
          return;
        }
        document.getElementById("player-form").reset();
        await loadPlayers();
      } catch (err) {
        showErr("player-err", "Failed to add non-winner.");
      }
    }

    loadWinners();
    loadPlayers();
  </script>
</body>
</html>
"#,
  ))
}

mod winners {
  use super::*;

  #[derive(Deserialize)]
  pub struct PostRequest {
    name: String,
    timestamp: i64,
    position: u8,
    winnings_cents: i64,
    pot_cents: i64,
  }

  pub async fn get(State(state): State<AppState>) -> Result<Json<Vec<WinnerSample>>, Error> {
    let winners = state.inner.winners.lock()?;
    Ok(Json(winners.clone()))
  }

  pub async fn post(
    State(state): State<AppState>,
    Json(req): Json<PostRequest>,
  ) -> Result<StatusCode, Error> {
    if !(1..=3).contains(&req.position) {
      return Err(Error::InvalidWinnerPosition(req.position));
    }
    if req.winnings_cents < 0 || req.pot_cents < 0 {
      return Err(Error::InvalidMoney);
    }

    let mut winners = state.inner.winners.lock()?;
    winners.push(WinnerSample {
      name: req.name,
      timestamp: req.timestamp,
      position: req.position,
      winnings_cents: req.winnings_cents,
      pot_cents: req.pot_cents,
    });

    Ok(StatusCode::CREATED)
  }
}

mod players {
  use super::*;

  #[derive(Deserialize)]
  pub struct PostRequest {
    name: String,
    timestamp: i64,
    buy_in_cents: i64,
    pot_cents: i64,
  }

  pub async fn get(State(state): State<AppState>) -> Result<Json<Vec<PlayerSample>>, Error> {
    let players = state.inner.players.lock()?;
    Ok(Json(players.clone()))
  }

  pub async fn post(
    State(state): State<AppState>,
    Json(req): Json<PostRequest>,
  ) -> Result<StatusCode, Error> {
    if req.buy_in_cents < 0 || req.pot_cents < 0 {
      return Err(Error::InvalidMoney);
    }

    let mut players = state.inner.players.lock()?;
    players.push(PlayerSample {
      name: req.name,
      timestamp: req.timestamp,
      buy_in_cents: req.buy_in_cents,
      pot_cents: req.pot_cents,
    });

    Ok(StatusCode::CREATED)
  }
}

#[instrument(name = "POKER")]
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
  let service = PokerService::new();
  let server = Server::new(addr, service).await?;

  info!("spinning up server...");
  let () = server.run().await?;
  info!("spinning down server...");

  Ok(())
}
