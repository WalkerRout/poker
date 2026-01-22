use serde::{Deserialize, Serialize};

use sqlx::postgres::PgPoolOptions;
use sqlx::{FromRow, PgPool};

use time::OffsetDateTime;

use uuid::Uuid;

#[derive(thiserror::Error, Debug)]
pub enum Error {
  #[error("sqlx error - {0}")]
  Sqlx(#[from] sqlx::Error),

  #[error("migration error - {0}")]
  Migration(#[from] sqlx::migrate::MigrateError),

  #[error("not found")]
  NotFound,
}

pub async fn migrate(pool: &PgPool) -> Result<(), Error> {
  sqlx::migrate!("./migrations").run(pool).await?;
  Ok(())
}

pub async fn connect(database_url: &str) -> Result<PgPool, Error> {
  let pool = PgPoolOptions::new()
    .max_connections(5)
    .connect(database_url)
    .await?;
  Ok(pool)
}

// non-blocking version of connect; the app depends on db but shouldnt go down because of it...
pub fn connect_lazy(database_url: &str) -> Result<PgPool, Error> {
  let pool = PgPoolOptions::new()
    .max_connections(5)
    .connect_lazy(database_url)?;
  Ok(pool)
}

// players

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Player {
  pub id: Uuid,
  pub first_name: String,
  pub last_name: String,
  pub created_at: OffsetDateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerStats {
  pub player: Player,
  pub total_games: i64,
  pub total_buy_in_cents: i64,
  pub total_winnings_cents: i64,
  pub net_cents: i64,
}

pub async fn create_player(
  pool: &PgPool,
  first_name: &str,
  last_name: &str,
) -> Result<Player, Error> {
  let id = Uuid::new_v4();
  let player = sqlx::query_as::<_, Player>(
    r#"
      INSERT INTO players (id, first_name, last_name)
      VALUES ($1, $2, $3)
      RETURNING id, first_name, last_name, created_at
    "#,
  )
  .bind(id)
  .bind(first_name)
  .bind(last_name)
  .fetch_one(pool)
  .await?;

  Ok(player)
}

pub async fn get_player(pool: &PgPool, id: Uuid) -> Result<Player, Error> {
  sqlx::query_as::<_, Player>(
    r#"SELECT id, first_name, last_name, created_at FROM players WHERE id = $1"#,
  )
  .bind(id)
  .fetch_optional(pool)
  .await?
  .ok_or(Error::NotFound)
}

pub async fn list_players(pool: &PgPool) -> Result<Vec<Player>, Error> {
  let players = sqlx::query_as::<_, Player>(
    r#"SELECT id, first_name, last_name, created_at FROM players ORDER BY first_name, last_name"#,
  )
  .fetch_all(pool)
  .await?;

  Ok(players)
}

pub async fn find_players_by_name(
  pool: &PgPool,
  first_name: &str,
  last_name: &str,
) -> Result<Vec<Player>, Error> {
  let players = sqlx::query_as::<_, Player>(
    r#"
      SELECT id, first_name, last_name, created_at 
      FROM players 
      WHERE LOWER(first_name) = LOWER($1) AND LOWER(last_name) = LOWER($2)
    "#,
  )
  .bind(first_name)
  .bind(last_name)
  .fetch_all(pool)
  .await?;

  Ok(players)
}

pub async fn delete_player(pool: &PgPool, id: Uuid) -> Result<(), Error> {
  let result = sqlx::query(r#"DELETE FROM players WHERE id = $1"#)
    .bind(id)
    .execute(pool)
    .await?;

  if result.rows_affected() == 0 {
    return Err(Error::NotFound);
  }

  Ok(())
}

// games

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Game {
  pub id: Uuid,
  pub started_at: OffsetDateTime,
  pub ended_at: OffsetDateTime,
  pub created_at: OffsetDateTime,
  pub updated_at: OffsetDateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct GameEntry {
  pub id: Uuid,
  pub game_id: Uuid,
  pub player_id: Uuid,
  pub buy_in_cents: i32,
  pub winnings_cents: i32,
  pub created_at: OffsetDateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameWithEntries {
  pub game: Game,
  pub entries: Vec<GameEntryWithPlayer>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameEntryWithPlayer {
  pub entry: GameEntry,
  pub player: Player,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateGameInput {
  pub started_at: OffsetDateTime,
  pub ended_at: OffsetDateTime,
  pub entries: Vec<CreateGameEntryInput>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateGameEntryInput {
  pub player_id: Uuid,
  pub buy_in_cents: i32,
  pub winnings_cents: i32,
}

pub async fn create_game(pool: &PgPool, input: CreateGameInput) -> Result<Game, Error> {
  let mut tx = pool.begin().await?;

  let id = Uuid::new_v4();
  let game = sqlx::query_as::<_, Game>(
    r#"
      INSERT INTO games (id, started_at, ended_at)
      VALUES ($1, $2, $3)
      RETURNING id, started_at, ended_at, created_at, updated_at
    "#,
  )
  .bind(id)
  .bind(input.started_at)
  .bind(input.ended_at)
  .fetch_one(&mut *tx)
  .await?;

  for entry in input.entries {
    let entry_id = Uuid::new_v4();
    sqlx::query(
      r#"
        INSERT INTO game_entries (id, game_id, player_id, buy_in_cents, winnings_cents)
        VALUES ($1, $2, $3, $4, $5)
      "#,
    )
    .bind(entry_id)
    .bind(id)
    .bind(entry.player_id)
    .bind(entry.buy_in_cents)
    .bind(entry.winnings_cents)
    .execute(&mut *tx)
    .await?;
  }

  tx.commit().await?;
  Ok(game)
}

#[derive(Debug, FromRow)]
struct GameEntryRow {
  entry_id: Uuid,
  game_id: Uuid,
  player_id: Uuid,
  buy_in_cents: i32,
  winnings_cents: i32,
  entry_created_at: OffsetDateTime,
  p_id: Uuid,
  first_name: String,
  last_name: String,
  player_created_at: OffsetDateTime,
}

pub async fn get_game_with_entries(pool: &PgPool, id: Uuid) -> Result<GameWithEntries, Error> {
  let game = sqlx::query_as::<_, Game>(
    r#"
      SELECT id, started_at, ended_at, created_at, updated_at
      FROM games WHERE id = $1
    "#,
  )
  .bind(id)
  .fetch_optional(pool)
  .await?
  .ok_or(Error::NotFound)?;

  let rows = sqlx::query_as::<_, GameEntryRow>(
    r#"
      SELECT 
        ge.id as entry_id, ge.game_id, ge.player_id, ge.buy_in_cents, ge.winnings_cents, ge.created_at as entry_created_at,
        p.id as p_id, p.first_name, p.last_name, p.created_at as player_created_at
      FROM game_entries ge
      JOIN players p ON p.id = ge.player_id
      WHERE ge.game_id = $1
      ORDER BY ge.winnings_cents DESC
    "#,
  )
  .bind(id)
  .fetch_all(pool)
  .await?;

  let entries = rows
    .into_iter()
    .map(|row| GameEntryWithPlayer {
      entry: GameEntry {
        id: row.entry_id,
        game_id: row.game_id,
        player_id: row.player_id,
        buy_in_cents: row.buy_in_cents,
        winnings_cents: row.winnings_cents,
        created_at: row.entry_created_at,
      },
      player: Player {
        id: row.p_id,
        first_name: row.first_name,
        last_name: row.last_name,
        created_at: row.player_created_at,
      },
    })
    .collect();

  Ok(GameWithEntries { game, entries })
}

pub async fn list_games(pool: &PgPool) -> Result<Vec<Game>, Error> {
  let games = sqlx::query_as::<_, Game>(
    r#"
      SELECT id, started_at, ended_at, created_at, updated_at
      FROM games ORDER BY started_at DESC
    "#,
  )
  .fetch_all(pool)
  .await?;

  Ok(games)
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateGameInput {
  pub started_at: Option<OffsetDateTime>,
  pub ended_at: Option<OffsetDateTime>,
  pub entries: Option<Vec<CreateGameEntryInput>>,
}

pub async fn update_game(pool: &PgPool, id: Uuid, input: UpdateGameInput) -> Result<Game, Error> {
  let mut tx = pool.begin().await?;

  let game = sqlx::query_as::<_, Game>(
    r#"
      UPDATE games SET
        started_at = COALESCE($2, started_at),
        ended_at = COALESCE($3, ended_at),
        updated_at = NOW()
      WHERE id = $1
      RETURNING id, started_at, ended_at, created_at, updated_at
    "#,
  )
  .bind(id)
  .bind(input.started_at)
  .bind(input.ended_at)
  .fetch_optional(&mut *tx)
  .await?
  .ok_or(Error::NotFound)?;

  if let Some(entries) = input.entries {
    sqlx::query(r#"DELETE FROM game_entries WHERE game_id = $1"#)
      .bind(id)
      .execute(&mut *tx)
      .await?;

    for entry in entries {
      let entry_id = Uuid::new_v4();
      sqlx::query(
        r#"
          INSERT INTO game_entries (id, game_id, player_id, buy_in_cents, winnings_cents)
          VALUES ($1, $2, $3, $4, $5)
        "#,
      )
      .bind(entry_id)
      .bind(id)
      .bind(entry.player_id)
      .bind(entry.buy_in_cents)
      .bind(entry.winnings_cents)
      .execute(&mut *tx)
      .await?;
    }
  }

  tx.commit().await?;
  Ok(game)
}

pub async fn delete_game(pool: &PgPool, id: Uuid) -> Result<(), Error> {
  let result = sqlx::query(r#"DELETE FROM games WHERE id = $1"#)
    .bind(id)
    .execute(pool)
    .await?;

  if result.rows_affected() == 0 {
    return Err(Error::NotFound);
  }

  Ok(())
}

// stats

#[derive(Debug, FromRow)]
struct StatsRow {
  total_games: i64,
  total_buy_in_cents: i64,
  total_winnings_cents: i64,
}

pub async fn get_player_stats(pool: &PgPool, player_id: Uuid) -> Result<PlayerStats, Error> {
  let player = get_player(pool, player_id).await?;

  let stats = sqlx::query_as::<_, StatsRow>(
    r#"
      SELECT 
        COUNT(DISTINCT game_id) as total_games,
        COALESCE(SUM(buy_in_cents), 0) as total_buy_in_cents,
        COALESCE(SUM(winnings_cents), 0) as total_winnings_cents
      FROM game_entries
      WHERE player_id = $1
    "#,
  )
  .bind(player_id)
  .fetch_one(pool)
  .await?;

  Ok(PlayerStats {
    player,
    total_games: stats.total_games,
    total_buy_in_cents: stats.total_buy_in_cents,
    total_winnings_cents: stats.total_winnings_cents,
    net_cents: stats.total_winnings_cents - stats.total_buy_in_cents,
  })
}

#[derive(Debug, FromRow)]
struct AllStatsRow {
  id: Uuid,
  first_name: String,
  last_name: String,
  created_at: OffsetDateTime,
  total_games: i64,
  total_buy_in_cents: i64,
  total_winnings_cents: i64,
}

pub async fn get_all_player_stats(pool: &PgPool) -> Result<Vec<PlayerStats>, Error> {
  let rows = sqlx::query_as::<_, AllStatsRow>(
    r#"
      SELECT 
        p.id, p.first_name, p.last_name, p.created_at,
        COUNT(DISTINCT ge.game_id) as total_games,
        COALESCE(SUM(ge.buy_in_cents), 0) as total_buy_in_cents,
        COALESCE(SUM(ge.winnings_cents), 0) as total_winnings_cents
      FROM players p
      LEFT JOIN game_entries ge ON ge.player_id = p.id
      GROUP BY p.id, p.first_name, p.last_name, p.created_at
      ORDER BY (COALESCE(SUM(ge.winnings_cents), 0) - COALESCE(SUM(ge.buy_in_cents), 0)) DESC
    "#,
  )
  .fetch_all(pool)
  .await?;

  Ok(
    rows
      .into_iter()
      .map(|row| PlayerStats {
        player: Player {
          id: row.id,
          first_name: row.first_name,
          last_name: row.last_name,
          created_at: row.created_at,
        },
        total_games: row.total_games,
        total_buy_in_cents: row.total_buy_in_cents,
        total_winnings_cents: row.total_winnings_cents,
        net_cents: row.total_winnings_cents - row.total_buy_in_cents,
      })
      .collect(),
  )
}