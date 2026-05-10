//! Conversions from alloy errors into [`atlas_core::ChainError`].

use atlas_core::error::{ChainError, RpcError};

pub(crate) fn map_transport_err<E: std::fmt::Display>(err: E) -> ChainError {
    ChainError::Rpc(RpcError::Transport(err.to_string()))
}

pub(crate) fn map_node_err<E: std::fmt::Display>(err: E) -> ChainError {
    ChainError::Rpc(RpcError::NodeError(err.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_transport_err_wraps_in_rpc_transport() {
        // Single-assertion Debug round-trip — no defensive `_ =>` arm to
        // leave uncovered when the happy path holds.
        assert_eq!(
            format!("{:?}", map_transport_err("connection reset")),
            r#"Rpc(Transport("connection reset"))"#
        );
    }

    #[test]
    fn map_node_err_wraps_in_rpc_node_error() {
        assert_eq!(
            format!("{:?}", map_node_err("execution reverted")),
            r#"Rpc(NodeError("execution reverted"))"#
        );
    }
}
