//! Client herdr-core (P2a) : transport JSON-RPC + méthodes de lecture typées.

pub mod client;
pub mod disposition;
pub mod port;
pub mod transport;
pub mod workspace_state;

pub use client::*;
pub use transport::*;
