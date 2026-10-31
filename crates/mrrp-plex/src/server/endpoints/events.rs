use std::fmt::Display;

use axum::{
    extract::{
        State,
        WebSocketUpgrade,
        ws::{
            CloseFrame,
            Message,
            WebSocket,
        },
    },
    response::Response,
};
use bytes::{
    BufMut,
    BytesMut,
};
use futures_util::{
    StreamExt,
    TryStreamExt,
    pin_mut,
};
use serde::Serialize;

use crate::{
    multiplexer::{
        Lagged,
        Multiplexer,
    },
    server::{
        ServerState,
        endpoints::AsCloseCode,
    },
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Axum(#[from] axum::Error),

    #[error(transparent)]
    CborDeserialize(#[from] ciborium::de::Error<std::io::Error>),

    #[error(transparent)]
    CborSerialize(#[from] ciborium::ser::Error<std::io::Error>),
}

impl AsCloseCode for Error {
    fn as_close_code(&self) -> u16 {
        // Internal error
        1011
    }
}

pub async fn get_events(State(state): State<ServerState>, ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(async move |socket: WebSocket| {
        let mut connection = Connection::new(socket);

        if let Err(error) = stream_events(state.multiplexer, &mut connection).await {
            tracing::error!(%error);
            connection.close_with_error(&error).await.ok();
        }
    })
}

async fn stream_events(multiplexer: Multiplexer, connection: &mut Connection) -> Result<(), Error> {
    let events = multiplexer
        .events()
        .inspect_err(|Lagged { lag }| tracing::warn!(?lag, "Event stream lagging behind"))
        .filter_map(async |result| result.ok());
    pin_mut!(events);

    while let Some(event) = events.next().await {
        connection.send_message(&event).await?;
    }

    Ok(())
}

#[derive(Debug)]
struct Connection {
    socket: WebSocket,
}

impl Connection {
    #[inline]
    pub fn new(socket: WebSocket) -> Self {
        Self { socket }
    }

    pub async fn send_message<T>(&mut self, message: &T) -> Result<(), Error>
    where
        T: Serialize,
    {
        let mut data = BytesMut::new();

        ciborium::into_writer(message, (&mut data).writer())?;
        self.socket.send(Message::Binary(data.freeze())).await?;

        Ok(())
    }

    pub async fn close_with_error<E>(mut self, error: &E) -> Result<(), Error>
    where
        E: Display + AsCloseCode,
    {
        self.socket
            .send(Message::Close(Some(CloseFrame {
                code: error.as_close_code(),
                reason: error.to_string().into(),
            })))
            .await?;
        Ok(())
    }
}
