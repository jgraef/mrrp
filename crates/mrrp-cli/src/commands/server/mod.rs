mod config;

use std::fmt::Debug;

use anyhow::Error;
use axum::Router;
use clap::Parser;
use mrrp_plex::multiplexer::{
    Multiplexer,
    events::EventBroadcaster,
    source::RtlSdr,
    stream::StreamHandle,
};
use tokio::net::TcpListener;
use tower_http::trace::TraceLayer;

pub use crate::commands::server::config::Config;
use crate::{
    commands::{
        Context,
        server::config::{
            DeviceSpecificReceiverConfig,
            ReceiverConfig,
            RtlSdrReceiverConfig,
        },
    },
    util::shutdown_signal,
};

#[derive(Clone, Debug, Parser)]
pub struct Args {
    /// Server listen address.
    ///
    /// This overrides the listen address set in the configuration file.
    #[clap(short, long)]
    listen_address: Option<String>,
}

pub async fn run(args: Args, context: Context) -> Result<(), Error> {
    let config = context.config.plex;

    let listen_address = &args
        .listen_address
        .as_deref()
        .unwrap_or(&config.listen_address);
    tracing::info!(?listen_address, "Listening to");
    let listener = TcpListener::bind(listen_address).await?;

    let shutdown = shutdown_signal();
    let multiplexer = Multiplexer::default();
    let event_broadcaster = EventBroadcaster::default();

    create_streams(&multiplexer, &event_broadcaster, config.receivers).await?;

    let router = Router::new()
        .nest(
            "/api/v1",
            mrrp_plex::server::router(multiplexer.clone(), event_broadcaster),
        )
        .layer(TraceLayer::new_for_http());

    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown.cancelled_owned())
        .await?;

    Ok(())
}

async fn create_streams(
    multiplexer: &Multiplexer,
    event_broadcaster: &EventBroadcaster,
    receivers: Vec<ReceiverConfig>,
) -> Result<(), Error> {
    let mut rtl_sdr_configs = Vec::with_capacity(receivers.len());

    for receiver in receivers {
        match receiver.device_specific {
            DeviceSpecificReceiverConfig::RtlSdr(rtl_sdr_receiver_config) => {
                rtl_sdr_configs.push((receiver.alias, rtl_sdr_receiver_config));
            }
        }
    }

    if !rtl_sdr_configs.is_empty() {
        for device_info in mrrp_rtl_sdr::enumerate_devices().await? {
            let matches = rtl_sdr_configs.extract_if(.., |(_alias, receiver_config)| {
                receiver_config.matches(&device_info)
            });

            // todo: make options configurable
            let device_options = Default::default();
            let reader_options = Default::default();

            let (matched, alias) = merge_rtl_sdr_matches(matches, &device_info);

            if matched {
                tracing::info!(?device_info, ?alias, "Matched RTL-SDR");

                match device_info.open(device_options).await {
                    Ok(device) => {
                        let stream_handle = StreamHandle::from_source(
                            RtlSdr::new(device, reader_options),
                            Some(event_broadcaster.clone()),
                            Default::default(),
                        );

                        tracing::info!(?alias, stream_id = ?stream_handle.stream_id(), "Opened RTL-SDR");

                        multiplexer.insert_stream(stream_handle);
                    }
                    Err(error) => {
                        tracing::error!(%error, "Failed to open device");
                    }
                }
            }

            /*let span = tracing::info_span!("watch_devices");
            let _join_handle = tokio::spawn(async move {
                //mrrp_rtl_sdr::enumerate::watch_devices()
                todo!();
            }.instrument(span));*/
        }
    }

    Ok(())
}

fn merge_rtl_sdr_matches(
    matches: impl Iterator<Item = (Option<String>, RtlSdrReceiverConfig)>,
    device_info: &mrrp_rtl_sdr::enumerate::DeviceInfo,
) -> (bool, Option<String>) {
    let mut matched = false;
    let mut output = None;
    let mut multiple_aliases = false;

    for (alias, _receiver_config) in matches {
        matched = true;

        if alias.is_some() {
            if output.is_none() {
                output = alias;
            }
            else {
                multiple_aliases = true;
            }
        }
    }

    if multiple_aliases {
        tracing::warn!(device = ?device_info, "Multiple aliases matches to device");
    }

    (matched, output)
}
