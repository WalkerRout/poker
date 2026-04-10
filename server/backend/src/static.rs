use std::env;
use std::path::Path;

use axum::Router;
use axum::http::{HeaderValue, header};
use axum::response::{IntoResponse, Redirect};
use axum::routing::get;

use tower_http::services::ServeDir;
use tower_http::set_header::SetResponseHeaderLayer;

use tower_layer::Layer;

use crate::state::AppState;

pub fn asset_router() -> Router<AppState> {
  const BACKEND_MANIFEST: &str = env!("CARGO_MANIFEST_DIR");
  let static_dir = Path::new(BACKEND_MANIFEST).join("static");
  let svc = ServeDir::new(static_dir)
    .precompressed_gzip()
    .precompressed_br();

  let svc = SetResponseHeaderLayer::if_not_present(
    header::CACHE_CONTROL,
    HeaderValue::from_static("public, max-age=31536000, immutable"),
  )
  .layer(svc);

  Router::new()
    .nest_service("/static", svc)
    .route("/favicon.ico", get(favicon))
    .route("/robots.txt", get(robots))
}

async fn favicon() -> impl IntoResponse {
  Redirect::permanent("/static/images/favicon.ico")
}

async fn robots() -> impl IntoResponse {
  Redirect::permanent("/static/robots.txt")
}
