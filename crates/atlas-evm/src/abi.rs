//! Minimal ABI encoding for the operations atlas-evm needs.
//!
//! Avoids depending on `alloy-contract` so the codec stays free of HTTP
//! client deps. We only need `transfer(address,uint256)` for ERC-20 transfers.

use alloy_primitives::{Address, U256};

/// Selector for `transfer(address,uint256)`: keccak256("transfer(address,uint256)")[..4].
pub const ERC20_TRANSFER_SELECTOR: [u8; 4] = [0xa9, 0x05, 0x9c, 0xbb];

/// ABI-encode a `transfer(to, amount)` call.
///
/// Layout: 4-byte selector ‖ 32-byte left-padded address ‖ 32-byte big-endian amount.
/// Total: 68 bytes.
pub fn encode_erc20_transfer(to: Address, amount: U256) -> Vec<u8> {
    let mut buf = Vec::with_capacity(68);
    buf.extend_from_slice(&ERC20_TRANSFER_SELECTOR);
    // address is 20 bytes; ABI encodes as 32-byte left-padded
    buf.extend_from_slice(&[0u8; 12]);
    buf.extend_from_slice(to.as_slice());
    // amount is 32-byte big-endian
    buf.extend_from_slice(&amount.to_be_bytes::<32>());
    buf
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::address;

    #[test]
    fn encodes_transfer_with_correct_selector() {
        let to = address!("0000000000000000000000000000000000000001");
        let amount = U256::from(100u64);
        let encoded = encode_erc20_transfer(to, amount);
        assert_eq!(encoded.len(), 68);
        assert_eq!(&encoded[..4], &ERC20_TRANSFER_SELECTOR);
        // address ends in 0x01
        assert_eq!(encoded[4 + 31], 0x01);
        // amount = 100, last byte
        assert_eq!(encoded[4 + 32 + 31], 100);
    }
}
