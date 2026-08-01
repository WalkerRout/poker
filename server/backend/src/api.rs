use axum::Json;
use axum::Router;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};

use serde::Deserialize;
use serde_json::json;

use tower_http::compression::CompressionLayer;

use tracing::error;

use uuid::Uuid;

use crate::db;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
  Router::new()
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
    .layer(CompressionLayer::new().br(true).gzip(true))
}

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
        error!("internal error - {}", self);
        let body = Json(json!({ "error": self.to_string() }));
        (StatusCode::INTERNAL_SERVER_ERROR, body).into_response()
      }
      Error::PlayerConflict(players) => {
        (StatusCode::CONFLICT, Json(players.clone())).into_response()
      }
    }
  }
}

mod players {
  use super::*;

  pub async fn list(State(state): State<AppState>) -> Result<Json<Vec<db::Player>>, Error> {
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
    State(state): State<AppState>,
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
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
  ) -> Result<Json<db::Player>, Error> {
    let player = db::get_player(&state.pool, id).await?;
    Ok(Json(player))
  }

  pub async fn delete(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
  ) -> Result<StatusCode, Error> {
    db::delete_player(&state.pool, id).await?;
    Ok(StatusCode::NO_CONTENT)
  }

  pub async fn stats(
    State(state): State<AppState>,
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
    State(state): State<AppState>,
    Json(req): Json<CheckNameRequest>,
  ) -> Result<Json<Vec<db::Player>>, Error> {
    let players = db::find_players_by_name(&state.pool, &req.first_name, &req.last_name).await?;
    Ok(Json(players))
  }
}

mod games {
  use super::*;

  pub async fn list(State(state): State<AppState>) -> Result<Json<Vec<db::GameWithPot>>, Error> {
    let games = db::list_games(&state.pool).await?;
    Ok(Json(games))
  }

  pub async fn create(
    State(state): State<AppState>,
    Json(input): Json<db::CreateGameInput>,
  ) -> Result<(StatusCode, Json<db::Game>), Error> {
    let game = db::create_game(&state.pool, input).await?;
    Ok((StatusCode::CREATED, Json(game)))
  }

  pub async fn get(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
  ) -> Result<Json<db::GameWithEntries>, Error> {
    let game = db::get_game_with_entries(&state.pool, id).await?;
    Ok(Json(game))
  }

  pub async fn update(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(input): Json<db::UpdateGameInput>,
  ) -> Result<Json<db::Game>, Error> {
    let game = db::update_game(&state.pool, id, input).await?;
    Ok(Json(game))
  }

  pub async fn delete(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
  ) -> Result<StatusCode, Error> {
    db::delete_game(&state.pool, id).await?;
    Ok(StatusCode::NO_CONTENT)
  }

  pub async fn settle(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
  ) -> Result<StatusCode, Error> {
    db::settle_game(&state.pool, id).await?;
    Ok(StatusCode::NO_CONTENT)
  }
}

mod stats {
  use super::*;

  pub async fn all(State(state): State<AppState>) -> Result<Json<Vec<db::PlayerLeaderboard>>, Error> {
    let stats = db::get_leaderboard(&state.pool).await?;
    Ok(Json(stats))
  }
}
