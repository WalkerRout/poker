use std::env;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};

use serde::Deserialize;
use serde_json::json;

use sqlx::PgPool;

use uuid::Uuid;

use tracing::{info, warn, instrument};
use tracing_subscriber::filter::LevelFilter;

mod db;

use lib_service::prelude::*;

#[derive(thiserror::Error, Debug)]
enum Error {
  #[error("database error - {0}")]
  Database(#[from] db::Error),

  #[error("player with this name already exists")]
  PlayerConflict(Vec<db::Player>),
}

impl IntoResponse for Error {
  fn into_response(self) -> Response {
    match &self {
      Error::Database(db::Error::GameLocked) => {
        let body = Json(json!({ "error": "game is settled and cannot be modified" }));
        (StatusCode::LOCKED, body).into_response()
      }
      Error::Database(db::Error::NotFound) => {
        let body = Json(json!({ "error": "not found" }));
        (StatusCode::NOT_FOUND, body).into_response()
      }
      Error::Database(_) => {
        tracing::error!("internal error - {}", self);
        let body = Json(json!({ "error": self.to_string() }));
        (StatusCode::INTERNAL_SERVER_ERROR, body).into_response()
      }
      Error::PlayerConflict(players) => {
        (StatusCode::CONFLICT, Json(players.clone())).into_response()
      }
    }
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
    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");

    info!("creating database pool (lazy connection)...");
    let pool = db::connect_lazy(&database_url)?;

    // try to run migrations, but don't crash if DB is unavailable
    info!("attempting migrations...");
    match db::migrate(&pool).await {
      Ok(()) => info!("migrations completed successfully"),
      Err(e) => warn!("migrations skipped (db may be unavailable): {}", e),
    }

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
      .route("/api/games/{id}/settle", post(games::settle))
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

  pub async fn list(State(state): State<Arc<AppState>>) -> Result<Json<Vec<db::GameWithPot>>, Error> {
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

  pub async fn settle(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
  ) -> Result<StatusCode, Error> {
    db::settle_game(&state.pool, id).await?;
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

async fn serve_ui() -> Html<&'static str> {
  Html(INDEX_HTML)
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
  <title>Poker</title>
  <style>
    * { box-sizing: border-box; margin: 0; padding: 0; }
    body { 
      font-family: system-ui, -apple-system, sans-serif; 
      background: #1a1a2e; 
      color: #eee; 
      padding: 16px;
      overflow-x: hidden;
    }
    .container { max-width: 600px; margin: 0 auto; }
    h1 { font-size: 1.5rem; margin-bottom: 16px; }
    h2 { font-size: 1.1rem; margin-bottom: 12px; display: flex; justify-content: space-between; align-items: center; }
    .card { background: #16213e; border-radius: 8px; padding: 16px; margin-bottom: 12px; }
    
    button { 
      background: #e94560; 
      color: white; 
      border: none; 
      padding: 8px 16px; 
      border-radius: 6px; 
      cursor: pointer;
      font-size: 0.9rem;
    }
    button:hover { background: #ff6b6b; }
    button.secondary { background: #0f3460; }
    button.secondary:hover { background: #1a4a7a; }
    button.small { padding: 4px 10px; font-size: 0.8rem; }
    button.remove { background: #666; padding: 4px 8px; }
    
    input, select { 
      padding: 8px 10px; 
      border-radius: 6px; 
      border: 1px solid #333; 
      background: #0f0f23; 
      color: #eee; 
      font-size: 0.9rem;
      width: 100%;
    }
    input:focus, select:focus { outline: 1px solid #e94560; }
    
    table { width: 100%; border-collapse: collapse; font-size: 0.9rem; }
    th, td { padding: 10px 8px; text-align: left; border-bottom: 1px solid #333; }
    th { background: #0f3460; font-weight: 500; }
    .positive { color: #4ade80; }
    .negative { color: #f87171; }
    .muted { color: #888; }
    
    .tabs { display: flex; gap: 8px; margin-bottom: 16px; }
    .tab { 
      padding: 8px 16px; 
      background: #0f3460; 
      border-radius: 6px; 
      cursor: pointer;
      font-size: 0.9rem;
    }
    .tab.active { background: #e94560; }
    
    .modal { 
      display: none; 
      position: fixed; 
      top: 0; left: 0; right: 0; bottom: 0;
      background: rgba(0,0,0,0.85); 
      justify-content: center; 
      align-items: flex-start;
      padding: 20px;
      overflow-y: auto;
    }
    .modal.active { display: flex; }
    .modal-content { 
      background: #16213e; 
      padding: 20px; 
      border-radius: 8px; 
      width: 100%;
      max-width: 400px;
      margin: auto;
    }
    .modal h2 { margin-bottom: 16px; }
    
    .form-row { margin-bottom: 12px; }
    .form-row label { display: block; font-size: 0.8rem; color: #aaa; margin-bottom: 4px; }
    .form-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 10px; }
    
    .entry-row { 
      display: grid; 
      grid-template-columns: 1fr 70px 70px 28px; 
      gap: 6px; 
      margin-bottom: 8px;
      align-items: center;
    }
    .entry-row input, .entry-row select { width: 100%; }
    
    .balance-check {
      font-size: 0.85rem;
      padding: 8px;
      border-radius: 6px;
      margin-top: 8px;
    }
    .balance-check.valid { background: rgba(74, 222, 128, 0.2); color: #4ade80; }
    .balance-check.invalid { background: rgba(248, 113, 113, 0.2); color: #f87171; }
    
    .actions { display: flex; gap: 8px; margin-top: 16px; }
    .actions button { flex: 1; }
    
    #entries-container { margin: 12px 0; }
    .empty-msg { color: #666; font-size: 0.85rem; padding: 8px 0; }
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
        <thead><tr><th>Player</th><th>Games</th><th>Net</th></tr></thead>
        <tbody id="stats-body"></tbody>
      </table>
    </div>

    <div id="games-tab" class="card" style="display:none">
      <h2>Games <button class="small" onclick="openGameModal()">+ New</button></h2>
      <table>
        <thead><tr><th>Date</th><th>Pot</th><th></th></tr></thead>
        <tbody id="games-body"></tbody>
      </table>
    </div>

    <div id="players-tab" class="card" style="display:none">
      <h2>Players <button class="small" onclick="openPlayerModal()">+ Add</button></h2>
      <table>
        <thead><tr><th>Name</th></tr></thead>
        <tbody id="players-body"></tbody>
      </table>
    </div>
  </div>

  <!-- Player Modal -->
  <div id="player-modal" class="modal">
    <div class="modal-content">
      <h2>Add Player</h2>
      <div class="form-row">
        <label>First Name</label>
        <input type="text" id="player-first" placeholder="John">
      </div>
      <div class="form-row">
        <label>Last Name</label>
        <input type="text" id="player-last" placeholder="Doe">
      </div>
      <div id="player-warning" style="color: #f59e0b; font-size: 0.85rem; margin: 8px 0; display: none;"></div>
      <div class="actions">
        <button class="secondary" onclick="closeModal('player-modal')">Cancel</button>
        <button onclick="createPlayer()">Add Player</button>
      </div>
    </div>
  </div>

  <!-- Game Modal -->
  <div id="game-modal" class="modal">
    <div class="modal-content">
      <h2 id="game-modal-title">New Game</h2>
      <div class="form-grid">
        <div class="form-row">
          <label>Start Time</label>
          <input type="time" id="game-start-time">
        </div>
        <div class="form-row">
          <label>End Time (optional)</label>
          <input type="time" id="game-end-time">
        </div>
      </div>
      <div class="form-row">
        <label>Date (defaults to today)</label>
        <input type="date" id="game-date">
      </div>
      
      <div style="margin-top: 16px;">
        <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 8px;">
          <label style="margin: 0;">Players</label>
          <button class="small secondary" onclick="addEntryRow()">+ Add</button>
        </div>
        <div id="entries-container">
          <div class="empty-msg">Click "+ Add" to add players</div>
        </div>
        <div id="balance-check" class="balance-check" style="display: none;"></div>
      </div>
      
      <div id="game-error" style="color: #f87171; font-size: 0.85rem; margin: 8px 0; display: none;"></div>
      <div class="actions">
        <button class="secondary" onclick="closeModal('game-modal')">Cancel</button>
        <button onclick="saveGame()">Save Game</button>
      </div>
    </div>
  </div>

  <!-- View Game Modal -->
  <div id="view-modal" class="modal">
    <div class="modal-content">
      <h2 id="view-modal-title">Game Details</h2>
      <div id="view-game-date" style="color: #aaa; font-size: 0.85rem; margin-bottom: 12px;"></div>
      <div id="view-balance" class="balance-check" style="margin-bottom: 12px;"></div>
      <table style="margin-bottom: 12px;">
        <thead><tr><th>Player</th><th>In</th><th>Out</th><th>Net</th></tr></thead>
        <tbody id="view-entries"></tbody>
      </table>
      <div class="actions">
        <button class="secondary" onclick="closeModal('view-modal')">Close</button>
      </div>
    </div>
  </div>

  <script>
    const NEW_GAME_PLACEHOLDER_ROWS = 4;
  
    let players = [];
    let editingGameId = null;
    let viewingGameId = null;

    async function api(path, opts = {}) {
      try {
        const res = await fetch('/api' + path, {
          headers: { 'Content-Type': 'application/json' },
          ...opts,
          body: opts.body ? JSON.stringify(opts.body) : undefined
        });
        if (res.status === 204) return null;
        if (!res.ok) {
          const text = await res.text();
          throw new Error(text || res.statusText);
        }
        return res.json();
      } catch (e) {
        console.error('API error:', e);
        throw e;
      }
    }

    function formatMoney(cents) {
      const dollars = Math.abs(cents / 100).toFixed(2);
      if (cents >= 0) return '$' + dollars;
      return '-$' + dollars;
    }

    function formatDate(d) {
      const date = new Date(d);
      return date.toLocaleDateString('en-US', { month: 'short', day: 'numeric' });
    }

    function showTab(name) {
      document.querySelectorAll('.tab').forEach(t => t.classList.remove('active'));
      document.querySelectorAll('[id$="-tab"]').forEach(t => t.style.display = 'none');
      document.querySelector(`[onclick="showTab('${name}')"]`).classList.add('active');
      document.getElementById(name + '-tab').style.display = 'block';
    }

    async function loadStats() {
      try {
        const stats = await api('/stats');
        if (!stats || stats.length === 0) {
          document.getElementById('stats-body').innerHTML = '<tr><td colspan="3" class="muted">No data yet</td></tr>';
          return;
        }
        // Check for duplicate names in stats
        document.getElementById('stats-body').innerHTML = stats.map(s => {
          const dupes = stats.filter(st => st.player.first_name === s.player.first_name && st.player.last_name === s.player.last_name);
          const name = dupes.length > 1 
            ? `${s.player.first_name} ${s.player.last_name} (${s.player.id.slice(-6)})`
            : `${s.player.first_name} ${s.player.last_name}`;
          return `
            <tr>
              <td>${name}</td>
              <td>${s.total_games}</td>
              <td class="${s.net_cents >= 0 ? 'positive' : 'negative'}">${formatMoney(s.net_cents)}</td>
            </tr>
          `;
        }).join('');
      } catch (e) {
        document.getElementById('stats-body').innerHTML = '<tr><td colspan="3" class="muted">Failed to load</td></tr>';
      }
    }

    async function loadGames() {
      try {
        const games = await api('/games');
        if (!games || games.length === 0) {
          document.getElementById('games-body').innerHTML = '<tr><td colspan="3" class="muted">No games yet</td></tr>';
          return;
        }
        document.getElementById('games-body').innerHTML = games.map(g => {
          const balanced = g.pot_cents === g.payout_cents;
          const icon = balanced ? '<span class="positive">✓</span>' : '<span class="negative">✗</span>';
          const inProgress = !g.ended_at ? ' <span class="muted" style="font-size:0.8rem">(in progress)</span>' : '';
          const actions = g.settled
            ? `<button class="small secondary" onclick="viewGame('${g.id}')">View</button>
               <span class="muted" style="font-size: 0.8rem; padding: 4px 8px;">Settled</span>`
            : `<button class="small secondary" onclick="viewGame('${g.id}')">View</button>
               <button class="small secondary" onclick="editGame('${g.id}')">Edit</button>`;
          return `
            <tr>
              <td>${icon} ${formatDate(g.started_at)}${inProgress}</td>
              <td>${formatMoney(g.pot_cents)}</td>
              <td>${actions}</td>
            </tr>
          `;
        }).join('');
      } catch (e) {
        document.getElementById('games-body').innerHTML = '<tr><td colspan="3" class="muted">Failed to load</td></tr>';
      }
    }

    async function loadPlayers() {
      try {
        players = await api('/players') || [];
        if (players.length === 0) {
          document.getElementById('players-body').innerHTML = '<tr><td class="muted">No players yet</td></tr>';
          return;
        }
        document.getElementById('players-body').innerHTML = players.map(p => {
          const dupes = players.filter(pl => pl.first_name === p.first_name && pl.last_name === p.last_name);
          const name = dupes.length > 1 
            ? `${p.first_name} ${p.last_name} (${p.id.slice(-6)})`
            : `${p.first_name} ${p.last_name}`;
          return `
            <tr>
              <td>${name}</td>
            </tr>
          `;
        }).join('');
      } catch (e) {
        document.getElementById('players-body').innerHTML = '<tr><td class="muted">Failed to load</td></tr>';
      }
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
      const first = document.getElementById('player-first').value.trim();
      const last = document.getElementById('player-last').value.trim();
      if (!first || !last) {
        document.getElementById('player-warning').textContent = 'Please enter both names';
        document.getElementById('player-warning').style.display = 'block';
        return;
      }
      try {
        const res = await fetch('/api/players', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ first_name: first, last_name: last, force })
        });
        if (res.status === 409) {
          document.getElementById('player-warning').textContent = 'Player exists. Click again to add anyway.';
          document.getElementById('player-warning').style.display = 'block';
          document.querySelector('#player-modal .actions button:last-child').onclick = () => createPlayer(true);
          return;
        }
        closeModal('player-modal');
        loadPlayers();
        loadStats();
      } catch (e) {
        document.getElementById('player-warning').textContent = 'Failed to add player';
        document.getElementById('player-warning').style.display = 'block';
      }
    }

    async function deletePlayer(id) {
      if (confirm('Delete this player?')) {
        try {
          await api('/players/' + id, { method: 'DELETE' });
          loadPlayers();
          loadStats();
        } catch (e) { alert('Failed to delete'); }
      }
    }

    function getTodayDate() {
      const now = new Date();
      const year = now.getFullYear();
      const month = String(now.getMonth() + 1).padStart(2, '0');
      const day = String(now.getDate()).padStart(2, '0');
      return `${year}-${month}-${day}`;
    }

    function getCurrentTime() {
      const now = new Date();
      return now.toTimeString().slice(0, 5);
    }

    function openGameModal() {
      editingGameId = null;
      document.getElementById('game-modal-title').textContent = 'New Game';
      document.getElementById('game-date').value = getTodayDate();
      document.getElementById('game-start-time').value = '';
      document.getElementById('game-end-time').value = '';
      document.getElementById('entries-container').innerHTML = '';
      document.getElementById('game-error').style.display = 'none';
      document.getElementById('balance-check').style.display = 'none';

      for (let i = 0; i < NEW_GAME_PLACEHOLDER_ROWS; i++) {
        addEntryRow('', 20, 0);
      }

      openModal('game-modal');
    }

    async function editGame(id) {
      editingGameId = id;
      try {
        const game = await api('/games/' + id);
        document.getElementById('game-modal-title').textContent = 'Edit Game';
        
        const startDate = new Date(game.game.started_at);
        
        // Extract local date for the date input
        const year = startDate.getFullYear();
        const month = String(startDate.getMonth() + 1).padStart(2, '0');
        const day = String(startDate.getDate()).padStart(2, '0');
        document.getElementById('game-date').value = `${year}-${month}-${day}`;
        document.getElementById('game-start-time').value = startDate.toTimeString().slice(0, 5);
        if (game.game.ended_at) {
          const endDate = new Date(game.game.ended_at);
          document.getElementById('game-end-time').value = endDate.toTimeString().slice(0, 5);
        } else {
          document.getElementById('game-end-time').value = '';
        }
        
        document.getElementById('entries-container').innerHTML = '';
        game.entries.forEach(e => addEntryRow(e.player.id, e.entry.buy_in_cents / 100, e.entry.winnings_cents / 100));
        if (game.entries.length === 0) {
          document.getElementById('entries-container').innerHTML = '<div class="empty-msg">Click "+ Add" to add players</div>';
          document.getElementById('balance-check').style.display = 'none';
        }
        document.getElementById('game-error').style.display = 'none';
        openModal('game-modal');
      } catch (e) { alert('Failed to load game'); }
    }

    async function viewGame(id) {
      viewingGameId = id;
      try {
        const game = await api('/games/' + id);
        const startDate = new Date(game.game.started_at);
        
        // Format date and time
        const dateStr = startDate.toLocaleDateString('en-US', { weekday: 'short', month: 'short', day: 'numeric' });
        const startTime = startDate.toLocaleTimeString('en-US', { hour: 'numeric', minute: '2-digit' });
        if (game.game.ended_at) {
          const endDate = new Date(game.game.ended_at);
          const endTime = endDate.toLocaleTimeString('en-US', { hour: 'numeric', minute: '2-digit' });
          document.getElementById('view-game-date').textContent = `${dateStr} \u2022 ${startTime} - ${endTime}`;
        } else {
          document.getElementById('view-game-date').textContent = `${dateStr} \u2022 ${startTime} - In progress`;
        }
        
        // Calculate totals
        let totalIn = 0, totalOut = 0;
        game.entries.forEach(e => {
          totalIn += e.entry.buy_in_cents;
          totalOut += e.entry.winnings_cents;
        });
        
        // Balance indicator
        const balanceEl = document.getElementById('view-balance');
        if (totalIn === totalOut) {
          balanceEl.className = 'balance-check valid';
          balanceEl.textContent = `\u2713 Balanced - Pot: ${formatMoney(totalIn)}`;
        } else {
          balanceEl.className = 'balance-check invalid';
          const diff = totalIn - totalOut;
          const msg = diff > 0 ? `${formatMoney(diff)} unpaid` : `${formatMoney(Math.abs(diff))} overpaid`;
          balanceEl.textContent = `\u2717 Unbalanced - Pot: ${formatMoney(totalIn)}, Paid: ${formatMoney(totalOut)} (${msg})`;
        }
        
        // Sort entries by winnings (winners first)
        const sorted = [...game.entries].sort((a, b) => b.entry.winnings_cents - a.entry.winnings_cents);
        
        // Build entries table
        document.getElementById('view-entries').innerHTML = sorted.map(e => {
          const net = e.entry.winnings_cents - e.entry.buy_in_cents;
          const netClass = net > 0 ? 'positive' : (net < 0 ? 'negative' : '');
          // Check for duplicate first names
          const dupes = players.filter(p => p.first_name === e.player.first_name);
          const name = dupes.length > 1 
            ? `${e.player.first_name} ${e.player.last_name} (${e.player.id.slice(-6)})`
            : `${e.player.first_name} ${e.player.last_name}`;
          return `
            <tr>
              <td>${name}</td>
              <td>${formatMoney(e.entry.buy_in_cents)}</td>
              <td>${formatMoney(e.entry.winnings_cents)}</td>
              <td class="${netClass}">${net >= 0 ? '+' : ''}${formatMoney(net)}</td>
            </tr>
          `;
        }).join('');
        
        openModal('view-modal');
      } catch (e) { alert('Failed to load game'); }
    }

    function getPlayerDisplayName(player) {
      // Check if there are other players with the same first name
      const dupes = players.filter(p => p.first_name === player.first_name);
      if (dupes.length > 1) {
        // Show last 6 chars of UUID (the random part at the end)
        const idSuffix = player.id.slice(-6);
        return `${player.first_name} ${player.last_name} (${idSuffix})`;
      }
      return player.first_name;
    }

    function addEntryRow(playerId = '', buyIn = 20, winnings = 0) {
      const empty = document.querySelector('#entries-container .empty-msg');
      if (empty) empty.remove();
      
      const div = document.createElement('div');
      div.className = 'entry-row';
      div.innerHTML = `
        <select class="entry-player" onchange="updateBalance()">
          <option value="">- SELECT -</option>
          ${players.map(p => `<option value="${p.id}" ${p.id === playerId ? 'selected' : ''}>${getPlayerDisplayName(p)}</option>`).join('')}
        </select>
        <input type="number" class="entry-buyin" placeholder="In" value="${buyIn}" oninput="updateBalance()">
        <input type="number" class="entry-winnings" placeholder="Out" value="${winnings}" oninput="updateBalance()">
        <button class="remove" onclick="removeEntry(this)">\u00d7</button>
      `;
      document.getElementById('entries-container').appendChild(div);
      updateBalance();
    }

    function updateBalance() {
      const rows = document.querySelectorAll('.entry-row');
      if (rows.length === 0) {
        document.getElementById('balance-check').style.display = 'none';
        return;
      }
      
      let totalIn = 0;
      let totalOut = 0;
      
      rows.forEach(row => {
        const buyIn = parseFloat(row.querySelector('.entry-buyin').value) || 0;
        const winnings = parseFloat(row.querySelector('.entry-winnings').value) || 0;
        totalIn += buyIn;
        totalOut += winnings;
      });
      
      const balanceEl = document.getElementById('balance-check');
      const diff = totalIn - totalOut;
      
      if (Math.abs(diff) < 0.01) {
        balanceEl.className = 'balance-check valid';
        balanceEl.textContent = `\u2713 Balanced - Pot: $${totalIn.toFixed(2)}`;
      } else {
        balanceEl.className = 'balance-check invalid';
        const remaining = diff > 0 ? `$${diff.toFixed(2)} left to pay out` : `$${Math.abs(diff).toFixed(2)} extra paid out`;
        balanceEl.textContent = `Pot: $${totalIn.toFixed(2)} | Paid: $${totalOut.toFixed(2)} | ${remaining}`;
      }
      balanceEl.style.display = 'block';
    }

    function removeEntry(btn) {
      btn.parentElement.remove();
      if (document.querySelectorAll('.entry-row').length === 0) {
        document.getElementById('entries-container').innerHTML = '<div class="empty-msg">Click "+ Add" to add players</div>';
        document.getElementById('balance-check').style.display = 'none';
      } else {
        updateBalance();
      }
    }

    async function saveGame() {
      const errEl = document.getElementById('game-error');
      errEl.style.display = 'none';
      
      const date = document.getElementById('game-date').value || getTodayDate();
      const startTime = document.getElementById('game-start-time').value;
      const endTime = document.getElementById('game-end-time').value;
      
      if (!startTime) {
        errEl.textContent = 'Please enter a start time';
        errEl.style.display = 'block';
        return;
      }
      
      const entries = Array.from(document.querySelectorAll('.entry-row')).map(row => ({
        player_id: row.querySelector('.entry-player').value,
        buy_in_cents: Math.round((parseFloat(row.querySelector('.entry-buyin').value) || 0) * 100),
        winnings_cents: Math.round((parseFloat(row.querySelector('.entry-winnings').value) || 0) * 100)
      })).filter(e => e.player_id);

      if (entries.length === 0) {
        errEl.textContent = 'Please add at least one player';
        errEl.style.display = 'block';
        return;
      }

      // Validate balance - warn but allow override via confirm
      const totalIn = entries.reduce((sum, e) => sum + e.buy_in_cents, 0);
      const totalOut = entries.reduce((sum, e) => sum + e.winnings_cents, 0);
      if (totalIn !== totalOut) {
        const diff = (totalIn - totalOut) / 100;
        const msg = diff > 0 
          ? `$${diff.toFixed(2)} left to pay out` 
          : `$${Math.abs(diff).toFixed(2)} overpaid`;
        if (!confirm(`Money doesn't balance (${msg}). Save anyway?`)) {
          return;
        }
      }

      const startedAt = new Date(date + 'T' + startTime + ':00').toISOString();
      const endedAt = endTime ? new Date(date + 'T' + endTime + ':00').toISOString() : null;

      const body = { started_at: startedAt, ended_at: endedAt, entries };

      try {
        if (editingGameId) {
          await api('/games/' + editingGameId, { method: 'PUT', body });
        } else {
          await api('/games', { method: 'POST', body });
        }
        closeModal('game-modal');
        loadGames();
        loadStats();
      } catch (e) {
        errEl.textContent = 'Failed to save game';
        errEl.style.display = 'block';
      }
    }

    async function deleteGame(id) {
      if (confirm('Delete this game?')) {
        try {
          await api('/games/' + id, { method: 'DELETE' });
          loadGames();
          loadStats();
        } catch (e) { alert('Failed to delete'); }
      }
    }

    loadStats();
    loadGames();
    loadPlayers();
  </script>
</body>
</html>
"#;
