mod cbor;
mod endpoints;

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
    server::endpoints::{
        events::get_events,
        stream::{
            create_stream,
            get_stream,
            list_streams,
        },
    },
};

pub fn router(multiplexer: Multiplexer) -> Router<()> {
    Router::new()
        .route("/stream", routing::put(create_stream).get(list_streams))
        .route("/stream/{stream_id}.cf32.zstd", routing::get(get_stream))
        .route("/events", routing::get(get_events))
        .with_state(ServerState::new(multiplexer))
}

#[derive(Clone, Debug)]
struct ServerState {
    multiplexer: Multiplexer,
}

impl ServerState {
    pub fn new(multiplexer: Multiplexer) -> Self {
        Self { multiplexer }
    }
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
