use std::io;
use std::net::SocketAddr;

use axum::Router;

use tokio::net::TcpListener;

use tracing::{info, instrument};

pub mod prelude {
  pub use super::{Server, ServerError, Service};
}

/// A `Service` provides an `axum::Router`, to be used internally by the `Server`
pub trait Service {
  /// Produce a router
  fn router(&self) -> Router;

  /// Merge `self` with `other`, where `other` is some generic `Service`
  ///
  /// Merging `Service`s by default will just merge the `axum::Router`s they produce
  fn merge<O>(self, other: O) -> impl Service
  where
    O: Service,
    Self: Sized,
  {
    // merge one router with the other, producing a type that impl Fn() -> Router
    // to take advantage of the blanket implementation
    move || self.router().merge(other.router())
  }
}

impl<F> Service for F
where
  F: Fn() -> Router,
{
  fn router(&self) -> Router {
    // just run the function...
    self()
  }
}

/// An error representation for `Server`
#[derive(thiserror::Error, Debug)]
pub enum ServerError {
  /// User provided an invalid socket address pair (address and port)
  #[error("invalid socket address - {0}")]
  InvalidSocketAddress(SocketAddr),

  /// `Server` crashed for some reason; `axum::serve` produced an error...
  #[error("server failed to serve - {0}")]
  FailedToServe(#[from] io::Error),
}

/// A service that runs on a specific address and port
pub struct Server<S> {
  service: S,
  listener: TcpListener,
}

impl<S> Server<S>
where
  S: Service,
{
  /// Construct a `Server` given socket address parts and a `Service`
  #[instrument(name = "SERVICE", skip(addr, service))]
  pub async fn new(addr: impl Into<SocketAddr>, service: S) -> Result<Self, ServerError> {
    let addr = addr.into();
    let listener = TcpListener::bind(addr)
      .await
      .map_err(|_| ServerError::InvalidSocketAddress(addr))?;
    if let Ok(local_addr) = listener.local_addr() {
      info!("listening on {local_addr}");
    }
    Ok(Self { service, listener })
  }

  /// Run the `Server` using the `Service` it was initialized with
  ///
  /// Accesses `self.service`s `Router` and opens it on a `tokio::net::TcpListener`
  /// that listens on the initially provided socket address
  #[instrument(name = "SERVICE", skip(self))]
  pub async fn run(self) -> Result<(), ServerError> {
    let app = self.service.router();
    info!("running server...");
    // todo maybe add some `error!` logging on error path...
    axum::serve(self.listener, app)
      .await
      .map_err(ServerError::FailedToServe)?;
    Ok(())
  }
}
