# mrrp

> I asked my cat for a project name :3

This is a collection of crates for SDR (Software Defined Radio) with Rust.

**IN DEVELOPMENT**

This is still under development. Don't expect anything to be in a usable state.

## Crates

This project consists of a number of different crates:

- `mrrp`: This just pulls in and re-exports some other crates to make it easier for application development and prototyping.
- `mrrp-adsb`: Mode-S / ADS-B demodulation and decoding.
- `mrrp-audio`: Read/Write WAVE files, play audio.
- `mrrp-cli`: Command-line interface
 - `rtl-sdr`: Interface RTL-SDR dongles. Debugging, `rtl_tcp` with fan-out.
 - `sat`: List satellite passes, correct doppler shift in files, etc.
- `mrrp-core`: Defines the main traits and types for DSP.
- `mrrp-file`: File formats (WIP)
- `mrrp-filter`: Signal filtering and filter synthesis.
- `mrrp-hamlib`: WIP hamlib rigctl client and server.
- `mrrp-modem`: General-purpose modulation & demodulations (e.g. FM).
- `mrrp-rtl-sdr`: From-scratch RTL-SDR driver in async Rust.
- `mrrp-rtl-tcp`: Client and server implementation of the `rtl_tcp` protocol.
- `mrrp-sat`: Satellite tracking
- `mrrp-sdr`: SDR GUI application
- `mrrp-sstv`: WIP SSTV encoder and decoder
- `mrrp-tui`: Deprecated TUI SDR app that initially started this project. ([Screenshot](https://media.githubusercontent.com/media/jgraef/mrrp/refs/heads/main/docs/mrrp-cli.png))
- `mrrp-util`: Useful utilities to use with `mrrp-core`.
- `mrrp-widgets`: egui widgets that are needed to display radio-related information in a GUI. Contains hardware-accelerated spectrum and waterfall renderers.
- `mrrp-xtask`: Development tools.
