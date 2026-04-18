use std::env;

use sqlx::PgPool;

use tracing::{info, warn};

use crate::db;

#[derive(thiserror::Error, Debug)]
pub enum Error {
  #[error("database error - {0}")]
  Database(#[from] db::Error),

  #[error("missing environment variable - {0}")]
  MissingEnv(#[from] env::VarError),
}

#[derive(Clone)]
pub struct AppState {
  pub pool: PgPool,
}

impl AppState {
  pub async fn new() -> Result<Self, Error> {
    let database_url = match env::var("DATABASE_URL") {
      Ok(url) => url,
      Err(_) => {
        warn!("DATABASE_URL not set, falling back to URL parts...");
        let host = env::var("DATABASE_HOST")?;
        let port = env::var("DATABASE_PORT").unwrap_or_else(|_| "5432".to_string());
        let name = env::var("DATABASE_NAME")?;
        let user = env::var("DATABASE_USER")?;
        let password = env::var("DATABASE_PASSWORD")?;
        format!(
          "postgres://{}:{}@{}:{}/{}",
          user, password, host, port, name
        )
      }
    };

    info!("creating database pool (lazy connection)...");
    let pool = db::connect_lazy(&database_url)?;

    info!("attempting migrations...");
    match db::migrate(&pool).await {
      Ok(()) => info!("migrations completed successfully"),
      Err(e) => warn!("migrations skipped (db may be unavailable) - {}", e),
    }

    Ok(Self { pool })
  }
}
