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
    multiplexer::{
        Multiplexer,
        events::EventBroadcaster,
        stream::StreamHandle,
    },
    protocol::StreamId,
    server::endpoints::streams::StreamIdOrAlias,
};

pub fn router(multiplexer: Multiplexer, events: EventBroadcaster) -> Router<()> {
    Router::new()
        .route("/streams", routing::get(endpoints::streams::list_streams))
        .route(
            "/streams/{stream_id}/ws",
            routing::get(endpoints::streams::get_stream_websocket),
        )
        .route("/events", routing::get(endpoints::events::get_events))
        .with_state(ServerState {
            multiplexer,
            events,
        })
}

#[derive(Clone, Debug)]
struct ServerState {
    multiplexer: Multiplexer,
    events: EventBroadcaster,
}

impl ServerState {
    pub fn get_stream(&self, stream_id: StreamIdOrAlias) -> Result<StreamHandle, ApiError> {
        let stream_handle = match &stream_id {
            StreamIdOrAlias::StreamId(stream_id) => self.multiplexer.get_stream(*stream_id),
            StreamIdOrAlias::Alias(_alias) => {
                // todo
                None
            }
        };

        stream_handle.ok_or_else(|| ApiError::StreamNotFound { stream_id })
    }
}

#[derive(Debug, Serialize, Deserialize, thiserror::Error)]
enum ApiError {
    #[error("Stream does not exist: {stream_id}")]
    StreamNotFound { stream_id: StreamIdOrAlias },

    #[error("Stream is closed: {stream_id}")]
    StreamClosed { stream_id: StreamId },
}

impl ApiError {
    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::StreamNotFound { stream_id: _ } => StatusCode::NOT_FOUND,
            Self::StreamClosed { stream_id: _ } => StatusCode::GONE,
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status_code = self.status_code();
        (status_code, Json(self)).into_response()
    }
}
