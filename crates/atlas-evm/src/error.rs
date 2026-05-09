//! Conversions from alloy errors into [`atlas_core::ChainError`].

use atlas_core::error::{ChainError, RpcError};

pub(crate) fn map_transport_err<E: std::fmt::Display>(err: E) -> ChainError {
    ChainError::Rpc(RpcError::Transport(err.to_string()))
}

pub(crate) fn map_node_err<E: std::fmt::Display>(err: E) -> ChainError {
    ChainError::Rpc(RpcError::NodeError(err.to_string()))
}
