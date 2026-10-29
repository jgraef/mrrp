mod cbor;
mod endpoints;

use std::fmt::Debug;

use axum::{
    Json,
    Router,
    http::StatusCode,
    response::{
        IntoResponse,
        Response,
    },
    routing,
};
use serde::{
    Deserialize,
    Serialize,
};

use crate::{
    multiplexer::Multiplexer,
    protocol::StreamId,
};

pub fn router(multiplexer: Multiplexer) -> Router<()> {
    Router::new()
        .route("/sources", routing::get(endpoints::sources::list_sources))
        .route(
            "/streams",
            routing::put(endpoints::streams::create_stream).get(endpoints::streams::list_streams),
        )
        .route(
            "/streams/{stream_id}.cf32.zstd",
            routing::get(endpoints::streams::get_stream),
        )
        .route("/events", routing::get(endpoints::events::get_events))
        .with_state(ServerState { multiplexer })
}

#[derive(Clone, Debug)]
struct ServerState {
    multiplexer: Multiplexer,
}

#[derive(Debug, Serialize, Deserialize, thiserror::Error)]
enum ApiError {
    #[error("Stream does not exist: {stream_id}")]
    StreamNotFound { stream_id: StreamId },

    #[error("Stream is closed: {stream_id}")]
    StreamClosed { stream_id: StreamId },
}

impl ApiError {
    pub fn status_code(&self) -> StatusCode {
        match self {
            ApiError::StreamNotFound { stream_id: _ } => StatusCode::NOT_FOUND,
            ApiError::StreamClosed { stream_id: _ } => StatusCode::GONE,
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status_code = self.status_code();
        (status_code, Json(self)).into_response()
    }
}
