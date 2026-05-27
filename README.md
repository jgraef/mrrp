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

