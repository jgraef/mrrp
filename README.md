# mrrp

> I asked my cat for a project name :3

This is a collection of crates for SDR (Software Defined Radio) with Rust.

**IN DEVELOPMENT**

This is still under a development and not usable yet.

## `mrrp`

Core crate that defines traits for streaming IQ asynchronously, and operating on them. Also contains tools for filter construction, file IO. There are some experimental demodulators. Some parts of this will likely be split into separate crates.

## `mrrp-cli`

Experimental terminal SDR app. Will likely not be developed for a while in favor of mrrp-sdr.

![mrrp-cli screenshot](https://media.githubusercontent.com/media/jgraef/mrrp/refs/heads/main/docs/screenshot.png)

## `mrrp-sdr`

Graphical SDR app.

## `mrrp-widgets`

egui widgets that are necessary for displaying radio-related things. Contains a GPU-rendered display for spectrum and waterfall.

## `mrrp-sat`

Satellite tracking

## `mrrp-adsb`

Mode-S a.k.a ADS-B demodulation and decoding for plane tracking.

TODO: Merge from adsbee repo.

## `mrrp-rigctl`

[hamlib rigctl](https://github.com/Hamlib/Hamlib) client and server.

TODO: Merge code

## Random TODOs

- Fix `Samples`/`SamplesMut` to actually be useful (`freeze`/`thaw`)
- `tracing`/`log` integration: some of our dependencies log using `log` crate, we want to forward that to tracing

## Planned features:

Just some features we want to implement eventually, but might forget if not noted down:

- Capture Rollback: Keep a ring-buffer of samples (on disk?) so that we can start a capture starting a few seconds in the past.
- Satellite overlay: Show transponder frequencies of satellites that are overhead.
- Lots of demodulators/decoders of course :3
- rpitx-like transmit (merge our BCM2711 code)
- Does our NanoVNA crate fit in here? Probably not.
