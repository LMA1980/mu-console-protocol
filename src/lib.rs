// SPDX-License-Identifier: LGPL-3.0-only WITH LGPL-3.0-linking-exception
//! MuConsole worker protocol.
//!
//! The UI module (main thread) and the data module (dedicated Web Worker)
//! exchange [`UiCommand`] and [`DataEvent`] messages as postcard bytes in
//! transferable `ArrayBuffer`s. Every frame starts with the protocol version
//! ([`PROTOCOL_VERSION`], a `u32`) so a stale worker or UI bundle is detected
//! and surfaced as a "reload required" banner instead of a decode panic.
//!
//! - [`model`]: identifiers, routes, commands and events;
//! - [`view`]: skin-agnostic view models with [`view::diff`] / [`view::apply`];
//! - [`dto`]: JSON DTOs of the REST + SSE API (`docs/api-contract.md`);
//! - [`config`]: `?api=` / `?scenario=` query parsing.
//!
//! This crate is `no_std` + `alloc` so it builds for `wasm32-unknown-unknown`
//! without pulling any browser bindings.
#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

pub mod codec;
pub mod config;
pub mod dto;
pub mod model;
pub mod view;

#[cfg(test)]
mod tests;

pub use codec::{decode, encode, peek_version, CodecError, PROTOCOL_VERSION};
pub use config::{WorkerConfig, LIVE_SCENARIO};
pub use model::*;
pub use view::*;
