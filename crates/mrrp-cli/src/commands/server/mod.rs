use anyhow::Error;
use axum::Router;
use clap::Parser;
use mrrp_plex::multiplexer::Multiplexer;
use tokio::net::TcpListener;
use tower_http::trace::TraceLayer;

use crate::{
    commands::Context,
    util::shutdown_signal,
};

#[derive(Clone, Debug, Parser)]
pub struct Args {
    #[clap(short, long, default_value = "localhost:7272")]
    listen_address: String,
}

pub async fn run(args: Args, _context: Context) -> Result<(), Error> {
    let listener = TcpListener::bind(&args.listen_address).await?;
    let shutdown = shutdown_signal();

    let multiplexer = Multiplexer::default();

    let router = Router::new()
        .nest("/api/v1", mrrp_plex::server::router(multiplexer.clone()))
        .layer(TraceLayer::new_for_http());

    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown.cancelled_owned())
        .await?;

    Ok(())
}
