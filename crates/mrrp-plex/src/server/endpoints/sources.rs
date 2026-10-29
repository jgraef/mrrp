use axum::extract::State;

use crate::{
    protocol::SourceListEntry,
    server::{
        ServerState,
        cbor::Cbor,
    },
};

pub async fn list_sources(State(state): State<ServerState>) -> Cbor<Vec<SourceListEntry>> {
    let buffer = vec![];
    //state.multiplexer.scan_sources(&mut buffer).await;
    Cbor(buffer)
}
