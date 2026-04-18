use tokio::fs;

use tracing::{error, info};

mod api;
mod db;
mod route;
mod server;
mod state;
mod r#static;
mod template;

use crate::server::Server;
use crate::state::AppState;

#[cfg(target_env = "musl")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

async fn show_visible_files() {
  if let Ok(mut files) = fs::read_dir("./").await {
    info!("visible files:");
    while let Ok(Some(file)) = files.next_entry().await {
      info!("- {}", file.path().display());
    }
  }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
  tracing_subscriber::fmt()
    .with_max_level(tracing_subscriber::filter::LevelFilter::INFO)
    .with_target(false)
    .with_thread_ids(true)
    .with_ansi(false)
    .init();

  log_panics::init();

  show_visible_files().await;

  info!("initializing app state...");
  let state = match AppState::new().await {
    Ok(state) => state,
    Err(e) => {
      error!("app state error - {e}");
      return;
    }
  };
  info!("initialized app state");

  info!("initializing poker_server...");
  match Server::new(state).await {
    Ok(server) => {
      if let Err(e) = server.run().await {
        error!("poker_server error - {e}");
      }
      info!("exiting poker_server...");
    }
    Err(e) => {
      error!("failed to initialize poker_server... - {e}");
      return;
    }
  }
}
