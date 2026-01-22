use std::collections::HashMap;
use std::env;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};

use serde::Deserialize;
use serde_json::json;

use sqlx::PgPool;

use uuid::Uuid;

use tracing::{info, instrument};
use tracing_subscriber::filter::LevelFilter;

mod db;

use lib_service::prelude::*;

#[derive(thiserror::Error, Debug)]
enum Error {
  #[error("database error - {0}")]
  Database(#[from] db::Error),

  #[error("player with this name already exists")]
  PlayerConflict(Vec<db::Player>),

  #[error("failed to read env var - {0}")]
  EnvVar(#[from] env::VarError),

  #[error("unauthorized access to UI")]
  UnauthorizedAccess,

  #[error("UI is disabled")]
  UiDisabled,
}

impl IntoResponse for Error {
  fn into_response(self) -> Response {
    let (status, message) = match &self {
      Error::Database(_) => {
        tracing::error!("internal error - {}", self);
        (StatusCode::INTERNAL_SERVER_ERROR, self.to_string())
      }
      Error::PlayerConflict(players) => {
        return (StatusCode::CONFLICT, Json(players.clone())).into_response();
      }
      _ => (StatusCode::BAD_REQUEST, self.to_string()),
    };

    let body = Json(json!({ "error": message }));
    (status, body).into_response()
  }
}

// insides must be Sync
#[derive(Clone)]
struct AppState {
  pool: PgPool,
}

struct PokerService {
  pool: PgPool,
}

impl PokerService {
  async fn new() -> Result<Self, Error> {
    let database_url = std::env::var("DATABASE_URL")?;

    info!("connecting to database...");
    let pool = db::connect(&database_url).await?;
    info!("connected to database successfully");

    info!("running migrations...");
    let () = db::migrate(&pool).await?;
    info!("migrations completed successfully");

    Ok(Self { pool })
  }
}

impl Service for PokerService {
  fn router(&self) -> Router {
    let state = AppState {
      pool: self.pool.clone(),
    };

    Router::new()
      .route("/", get(serve_ui))
      .route("/api/players", get(players::list).post(players::create))
      .route(
        "/api/players/{id}",
        get(players::get).delete(players::delete),
      )
      .route("/api/players/{id}/stats", get(players::stats))
      .route("/api/players/check", post(players::check_name))
      .route("/api/games", get(games::list).post(games::create))
      .route(
        "/api/games/{id}",
        get(games::get).put(games::update).delete(games::delete),
      )
      .route("/api/stats", get(stats::all))
      .with_state(Arc::new(state))
  }
}

mod players {
  use super::*;

  pub async fn list(State(state): State<Arc<AppState>>) -> Result<Json<Vec<db::Player>>, Error> {
    let players = db::list_players(&state.pool).await?;
    Ok(Json(players))
  }

  #[derive(Deserialize)]
  pub struct CreateRequest {
    first_name: String,
    last_name: String,
    force: Option<bool>,
  }

  pub async fn create(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateRequest>,
  ) -> Result<Response, Error> {
    if req.force != Some(true) {
      let existing =
        db::find_players_by_name(&state.pool, &req.first_name, &req.last_name).await?;
      if !existing.is_empty() {
        return Err(Error::PlayerConflict(existing));
      }
    }

    let player = db::create_player(&state.pool, &req.first_name, &req.last_name).await?;
    Ok((StatusCode::CREATED, Json(player)).into_response())
  }

  pub async fn get(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
  ) -> Result<Json<db::Player>, Error> {
    let player = db::get_player(&state.pool, id).await?;
    Ok(Json(player))
  }

  pub async fn delete(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
  ) -> Result<StatusCode, Error> {
    db::delete_player(&state.pool, id).await?;
    Ok(StatusCode::NO_CONTENT)
  }

  pub async fn stats(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
  ) -> Result<Json<db::PlayerStats>, Error> {
    let stats = db::get_player_stats(&state.pool, id).await?;
    Ok(Json(stats))
  }

  #[derive(Deserialize)]
  pub struct CheckNameRequest {
    first_name: String,
    last_name: String,
  }

  pub async fn check_name(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CheckNameRequest>,
  ) -> Result<Json<Vec<db::Player>>, Error> {
    let players =
      db::find_players_by_name(&state.pool, &req.first_name, &req.last_name).await?;
    Ok(Json(players))
  }
}

mod games {
  use super::*;

  pub async fn list(State(state): State<Arc<AppState>>) -> Result<Json<Vec<db::Game>>, Error> {
    let games = db::list_games(&state.pool).await?;
    Ok(Json(games))
  }

  pub async fn create(
    State(state): State<Arc<AppState>>,
    Json(input): Json<db::CreateGameInput>,
  ) -> Result<(StatusCode, Json<db::Game>), Error> {
    let game = db::create_game(&state.pool, input).await?;
    Ok((StatusCode::CREATED, Json(game)))
  }

  pub async fn get(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
  ) -> Result<Json<db::GameWithEntries>, Error> {
    let game = db::get_game_with_entries(&state.pool, id).await?;
    Ok(Json(game))
  }

  pub async fn update(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Json(input): Json<db::UpdateGameInput>,
  ) -> Result<Json<db::Game>, Error> {
    let game = db::update_game(&state.pool, id, input).await?;
    Ok(Json(game))
  }

  pub async fn delete(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
  ) -> Result<StatusCode, Error> {
    db::delete_game(&state.pool, id).await?;
    Ok(StatusCode::NO_CONTENT)
  }
}

mod stats {
  use super::*;

  pub async fn all(
    State(state): State<Arc<AppState>>,
  ) -> Result<Json<Vec<db::PlayerStats>>, Error> {
    let stats = db::get_all_player_stats(&state.pool).await?;
    Ok(Json(stats))
  }
}

async fn serve_ui(
  Query(params): Query<HashMap<String, String>>,
) -> Result<Html<&'static str>, Error> {
  let password = env::var("UI_PASSWORD")?;

  if password.is_empty() {
    return Err(Error::UiDisabled);
  }

  if params.get("password") != Some(&password) {
    return Err(Error::UnauthorizedAccess);
  }

  Ok(Html(INDEX_HTML))
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
  let service = PokerService::new().await?;
  let server = Server::new(addr, service).await?;

  info!("spinning up server...");
  let () = server.run().await?;
  info!("spinning down server...");

  Ok(())
}

const INDEX_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Poker Tracker</title>
  <style>
    * { box-sizing: border-box; margin: 0; padding: 0; }
    body { font-family: system-ui, sans-serif; background: #1a1a2e; color: #eee; padding: 20px; }
    h1, h2 { margin-bottom: 20px; }
    .container { max-width: 900px; margin: 0 auto; }
    .card { background: #16213e; border-radius: 8px; padding: 20px; margin-bottom: 20px; }
    button { background: #e94560; color: white; border: none; padding: 10px 20px; border-radius: 4px; cursor: pointer; margin: 5px; }
    button:hover { background: #ff6b6b; }
    button.secondary { background: #0f3460; }
    button.secondary:hover { background: #1a4a7a; }
    input, select { padding: 10px; border-radius: 4px; border: 1px solid #333; background: #0f0f23; color: #eee; margin: 5px; width: 200px; }
    table { width: 100%; border-collapse: collapse; margin-top: 10px; }
    th, td { padding: 12px; text-align: left; border-bottom: 1px solid #333; }
    th { background: #0f3460; }
    .positive { color: #4ade80; }
    .negative { color: #f87171; }
    .modal { display: none; position: fixed; top: 0; left: 0; width: 100%; height: 100%; background: rgba(0,0,0,0.8); justify-content: center; align-items: center; }
    .modal.active { display: flex; }
    .modal-content { background: #16213e; padding: 30px; border-radius: 8px; max-width: 500px; width: 90%; }
    .entry-row { display: flex; gap: 10px; margin: 10px 0; align-items: center; }
    .entry-row input, .entry-row select { flex: 1; }
    #entries-container { max-height: 300px; overflow-y: auto; }
    .tabs { display: flex; gap: 10px; margin-bottom: 20px; }
    .tab { padding: 10px 20px; background: #0f3460; border-radius: 4px; cursor: pointer; }
    .tab.active { background: #e94560; }
  </style>
</head>
<body>
  <div class="container">
    <h1>Poker Tracker</h1>
    
    <div class="tabs">
      <div class="tab active" onclick="showTab('stats')">Stats</div>
      <div class="tab" onclick="showTab('games')">Games</div>
      <div class="tab" onclick="showTab('players')">Players</div>
    </div>

    <div id="stats-tab" class="card">
      <h2>Leaderboard</h2>
      <table>
        <thead><tr><th>Player</th><th>Games</th><th>Buy-ins</th><th>Winnings</th><th>Net</th></tr></thead>
        <tbody id="stats-body"></tbody>
      </table>
    </div>

    <div id="games-tab" class="card" style="display:none">
      <h2>Games <button onclick="openGameModal()">+ New Game</button></h2>
      <table>
        <thead><tr><th>Date</th><th>Duration</th><th>Players</th><th>Actions</th></tr></thead>
        <tbody id="games-body"></tbody>
      </table>
    </div>

    <div id="players-tab" class="card" style="display:none">
      <h2>Players <button onclick="openPlayerModal()">+ Add Player</button></h2>
      <table>
        <thead><tr><th>Name</th><th>Created</th><th>Actions</th></tr></thead>
        <tbody id="players-body"></tbody>
      </table>
    </div>
  </div>

  <!-- Player Modal -->
  <div id="player-modal" class="modal">
    <div class="modal-content">
      <h2>Add Player</h2>
      <input type="text" id="player-first" placeholder="First name">
      <input type="text" id="player-last" placeholder="Last name">
      <div id="player-warning" style="color: #f59e0b; margin: 10px 0; display: none;"></div>
      <div style="margin-top: 20px;">
        <button onclick="createPlayer()">Create</button>
        <button class="secondary" onclick="closeModal('player-modal')">Cancel</button>
      </div>
    </div>
  </div>

  <!-- Game Modal -->
  <div id="game-modal" class="modal">
    <div class="modal-content">
      <h2 id="game-modal-title">New Game</h2>
      <input type="datetime-local" id="game-start" placeholder="Start time">
      <input type="datetime-local" id="game-end" placeholder="End time">
      <h3 style="margin: 20px 0 10px;">Players</h3>
      <div id="entries-container"></div>
      <button class="secondary" onclick="addEntryRow()">+ Add Player</button>
      <div style="margin-top: 20px;">
        <button onclick="saveGame()">Save</button>
        <button class="secondary" onclick="closeModal('game-modal')">Cancel</button>
      </div>
    </div>
  </div>

  <!-- Confirm Modal -->
  <div id="confirm-modal" class="modal">
    <div class="modal-content">
      <p id="confirm-text"></p>
      <div style="margin-top: 20px;">
        <button id="confirm-yes">Yes</button>
        <button class="secondary" onclick="closeModal('confirm-modal')">No</button>
      </div>
    </div>
  </div>

  <script>
    let players = [];
    let editingGameId = null;

    async function api(path, opts = {}) {
      const res = await fetch('/api' + path, {
        headers: { 'Content-Type': 'application/json' },
        ...opts,
        body: opts.body ? JSON.stringify(opts.body) : undefined
      });
      if (res.status === 204) return null;
      return res.json();
    }

    function formatCents(c) {
      const dollars = (c / 100).toFixed(2);
      return c >= 0 ? `$${dollars}` : `-$${Math.abs(dollars).toFixed(2)}`;
    }

    function formatDate(d) {
      return new Date(d).toLocaleDateString();
    }

    function showTab(name) {
      document.querySelectorAll('.tab').forEach(t => t.classList.remove('active'));
      document.querySelectorAll('[id$="-tab"]').forEach(t => t.style.display = 'none');
      document.querySelector(`[onclick="showTab('${name}')"]`).classList.add('active');
      document.getElementById(name + '-tab').style.display = 'block';
    }

    async function loadStats() {
      const stats = await api('/stats');
      document.getElementById('stats-body').innerHTML = stats.map(s => `
        <tr>
          <td>${s.player.first_name} ${s.player.last_name}</td>
          <td>${s.total_games}</td>
          <td>${formatCents(s.total_buy_in_cents)}</td>
          <td>${formatCents(s.total_winnings_cents)}</td>
          <td class="${s.net_cents >= 0 ? 'positive' : 'negative'}">${formatCents(s.net_cents)}</td>
        </tr>
      `).join('');
    }

    async function loadGames() {
      const games = await api('/games');
      document.getElementById('games-body').innerHTML = games.map(g => `
        <tr>
          <td>${formatDate(g.started_at)}</td>
          <td>${Math.round((new Date(g.ended_at) - new Date(g.started_at)) / 60000)} min</td>
          <td>-</td>
          <td>
            <button class="secondary" onclick="editGame('${g.id}')">Edit</button>
            <button onclick="deleteGame('${g.id}')">Delete</button>
          </td>
        </tr>
      `).join('');
    }

    async function loadPlayers() {
      players = await api('/players');
      document.getElementById('players-body').innerHTML = players.map(p => `
        <tr>
          <td>${p.first_name} ${p.last_name}</td>
          <td>${formatDate(p.created_at)}</td>
          <td><button onclick="deletePlayer('${p.id}')">Delete</button></td>
        </tr>
      `).join('');
    }

    function openModal(id) { document.getElementById(id).classList.add('active'); }
    function closeModal(id) { document.getElementById(id).classList.remove('active'); }

    function openPlayerModal() {
      document.getElementById('player-first').value = '';
      document.getElementById('player-last').value = '';
      document.getElementById('player-warning').style.display = 'none';
      openModal('player-modal');
    }

    async function createPlayer(force = false) {
      const first = document.getElementById('player-first').value;
      const last = document.getElementById('player-last').value;
      const res = await fetch('/api/players', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ first_name: first, last_name: last, force })
      });
      if (res.status === 409) {
        document.getElementById('player-warning').textContent = 'Player with this name exists. Click Create again to add anyway.';
        document.getElementById('player-warning').style.display = 'block';
        document.querySelector('#player-modal button').onclick = () => createPlayer(true);
        return;
      }
      closeModal('player-modal');
      loadPlayers();
      loadStats();
    }

    async function deletePlayer(id) {
      if (confirm('Delete this player?')) {
        await api('/players/' + id, { method: 'DELETE' });
        loadPlayers();
        loadStats();
      }
    }

    function openGameModal() {
      editingGameId = null;
      document.getElementById('game-modal-title').textContent = 'New Game';
      document.getElementById('game-start').value = '';
      document.getElementById('game-end').value = '';
      document.getElementById('entries-container').innerHTML = '';
      addEntryRow();
      addEntryRow();
      openModal('game-modal');
    }

    async function editGame(id) {
      editingGameId = id;
      const game = await api('/games/' + id);
      document.getElementById('game-modal-title').textContent = 'Edit Game';
      document.getElementById('game-start').value = game.game.started_at.slice(0, 16);
      document.getElementById('game-end').value = game.game.ended_at.slice(0, 16);
      document.getElementById('entries-container').innerHTML = '';
      game.entries.forEach(e => addEntryRow(e.player.id, e.entry.buy_in_cents / 100, e.entry.winnings_cents / 100));
      openModal('game-modal');
    }

    function addEntryRow(playerId = '', buyIn = 20, winnings = 0) {
      const div = document.createElement('div');
      div.className = 'entry-row';
      div.innerHTML = `
        <select class="entry-player">
          <option value="">Select player</option>
          ${players.map(p => `<option value="${p.id}" ${p.id === playerId ? 'selected' : ''}>${p.first_name} ${p.last_name}</option>`).join('')}
        </select>
        <input type="number" class="entry-buyin" placeholder="Buy-in $" value="${buyIn}">
        <input type="number" class="entry-winnings" placeholder="Winnings $" value="${winnings}">
        <button class="secondary" onclick="this.parentElement.remove()">×</button>
      `;
      document.getElementById('entries-container').appendChild(div);
    }

    async function saveGame() {
      const entries = Array.from(document.querySelectorAll('.entry-row')).map(row => ({
        player_id: row.querySelector('.entry-player').value,
        buy_in_cents: Math.round(parseFloat(row.querySelector('.entry-buyin').value || 0) * 100),
        winnings_cents: Math.round(parseFloat(row.querySelector('.entry-winnings').value || 0) * 100)
      })).filter(e => e.player_id);

      const body = {
        started_at: new Date(document.getElementById('game-start').value).toISOString(),
        ended_at: new Date(document.getElementById('game-end').value).toISOString(),
        entries
      };

      if (editingGameId) {
        await api('/games/' + editingGameId, { method: 'PUT', body });
      } else {
        await api('/games', { method: 'POST', body });
      }
      closeModal('game-modal');
      loadGames();
      loadStats();
    }

    async function deleteGame(id) {
      if (confirm('Delete this game?')) {
        await api('/games/' + id, { method: 'DELETE' });
        loadGames();
        loadStats();
      }
    }

    loadStats();
    loadGames();
    loadPlayers();
  </script>
</body>
</html>
"#;