//! Lugh Core: the runtime for the Lugh agent programming language.
//!
//! The `native` feature (on by default) adds everything that touches the
//! operating system and network: the HTTP, SQLite and MCP adapters, the LLM
//! client and `lugh plan`. Without it the crate builds for the browser
//! (wasm32-unknown-unknown), where the playground runs it.

pub mod adapters;
pub mod clock;
pub mod info;
pub mod parser;
pub mod playground;
pub mod program;
pub mod runtime;
pub mod stdlib;
pub mod web;

#[cfg(feature = "native")]
pub mod mcp;
#[cfg(feature = "native")]
pub mod plan;

#[cfg(target_arch = "wasm32")]
mod wasm;
