//! hub — one channel for email / GitHub / project devlog / chat, triaged by a
//! bounded `claude -p` call, answered on the channel it came from.
//!
//! Rust port of the Node prototype (kept as an oracle until parity is proven).
//! The sqlite schema and `hub.config.json` are byte-compatible with it on
//! purpose: both binaries can run against the same `data/hub.sqlite`.

pub mod act;
pub mod adapters;
pub mod config;
pub mod db;
pub mod exec;
pub mod live;
pub mod logging;
pub mod outbound;
pub mod pipeline;
pub mod policy;
pub mod portal;
pub mod redaction;
pub mod sessions;
pub mod triage;
pub mod web;
