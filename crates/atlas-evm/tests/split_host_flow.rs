//! Hybrid usage tests: prove the SDK works as both a single-process
//! library and a split-host (server-builds, client-signs) integration.
//!
//! Atlas's chain-service split is into a pure codec + RPC reader / fee
//! estimator / broadcaster. That split is only valuable if it actually
//! decomposes across a wire boundary. These tests verify two claims:
//!
//! 1. **Direct mode.** A single process can hold the full
//!    [`EvmChainService`], sign with a local signer, and broadcast,
//!    end-to-end through the orchestrator's `transfer` method.
//! 2. **Split-host mode.** A *server* with no signer can build the
//!    [`UnsignedBundle`], a *client* with no reader/estimator can sign
//!    and broadcast — talking only via JSON over the wire types
//!    (`UnsignedTransaction`, `SigningRequest`, `SigningResponse`).
//!
//! Both halves share zero process state; they each connect to a
//! distinct mocked alloy provider. The signer never appears in the
//! server's scope and the reader/estimator never appear in the
//! client's scope, which is the architectural property we care about.

use alloy_consensus::transaction::SignerRecoverable;
use alloy_consensus::TxEnvelope;
use alloy_eips::eip2718::Decodable2718;
use alloy_primitives::{Address, B256};
use alloy_provider::mock::Asserter;
use alloy_provider::ProviderBuilder;
use alloy_rpc_types_eth::FeeHistory;
use atlas_core::amount::RawAmount;
use atlas_core::id::{AccountRef, AddressRef, AssetInstanceId, NetworkId, SignerId};
use atlas_core::service::{ChainCodec, ChainService};
use atlas_core::signing::{SignerProvider, SigningResponse};
use atlas_core::transaction::{TransferIntent, UnsignedBundle};
use atlas_evm::service::EvmChainService;
use atlas_signer_localkey::LocalKeySigner;
use num_bigint::BigInt;
use std::str::FromStr;

const TEST_KEY: [u8; 32] = [
    0x4c, 0x0d, 0xa3, 0xc7, 0xe6, 0x09, 0xa1, 0x6e, 0x42, 0x06, 0x4e, 0x9c, 0x16, 0x1c, 0x32, 0x06,
    0x16, 0x9c, 0x32, 0x06, 0x9c, 0x32, 0x06, 0x9c, 0x32, 0x06, 0x9c, 0x32, 0x06, 0x9c, 0x32, 0x06,
];

fn mk_signer() -> LocalKeySigner {
    LocalKeySigner::from_bytes(SignerId::from_str("test").unwrap(), TEST_KEY).unwrap()
}

fn native_intent() -> TransferIntent {
    TransferIntent {
        asset_instance_id: AssetInstanceId::from_str("eip155:1/native:eth").unwrap(),
        to: AddressRef::from_str("0x0000000000000000000000000000000000000002").unwrap(),
        amount: RawAmount::new(BigInt::from(123_456u64), 18).unwrap(),
    }
}

/// Push the canned `eth_getTransactionCount` + `eth_feeHistory`
/// responses that `prepare_unsigned_bundle` consumes (in that order).
fn push_read_and_estimate(asserter: &Asserter) {
    asserter.push_success(&alloy_primitives::U64::from(7u64));
    asserter.push_success(&FeeHistory {
        base_fee_per_gas: vec![1_000_000_000u128, 1_000_000_000u128],
        gas_used_ratio: vec![0.5],
        base_fee_per_blob_gas: Vec::new(),
        blob_gas_used_ratio: Vec::new(),
        oldest_block: 1,
        reward: Some(vec![vec![5_000_000u128]; 3]),
    });
}

#[tokio::test]
async fn direct_mode_in_process_transfer() {
    // The "everything-on-one-machine" path: one provider, one service,
    // local signer in scope, full ChainService::transfer flow.
    let asserter = Asserter::new();
    push_read_and_estimate(&asserter);
    let expected_hash = B256::repeat_byte(0xcd);
    asserter.push_success(&expected_hash);

    let provider = ProviderBuilder::new()
        .disable_recommended_fillers()
        .connect_mocked_client(asserter);
    let signer = mk_signer();
    let service = EvmChainService::new(provider, NetworkId::from_str("eip155:1").unwrap(), 1, true);

    let result = service
        .transfer(
            native_intent(),
            AccountRef::from_str(&signer.address()).unwrap(),
            &signer,
        )
        .await
        .unwrap();
    assert_eq!(result.tx_hash, format!("{:#x}", expected_hash));
}

#[tokio::test]
async fn split_host_server_builds_client_signs_and_broadcasts() {
    // ── Server side ────────────────────────────────────────────────
    // Holds: codec + reader + fee estimator (via the orchestrator), plus
    // its own RPC provider for nonce / fee fetches.
    // Critically, the local signer is *not* in scope at this layer.
    let server_asserter = Asserter::new();
    push_read_and_estimate(&server_asserter);
    let server_provider = ProviderBuilder::new()
        .disable_recommended_fillers()
        .connect_mocked_client(server_asserter);
    let server_service = EvmChainService::new(
        server_provider,
        NetworkId::from_str("eip155:1").unwrap(),
        1,
        true,
    );

    // We still need the client's address up front (the v1 AccountRef
    // contract — see ChainService::transfer docs). In a real backend
    // this comes from session state, not from a signer.
    let client_signer = mk_signer();
    let client_address = client_signer.address();
    let account = AccountRef::from_str(&client_address).unwrap();

    let bundle: UnsignedBundle = server_service
        .prepare_unsigned_bundle(native_intent(), account)
        .await
        .expect("server build");

    // ── Wire boundary ──────────────────────────────────────────────
    // The bundle is JSON-serializable end-to-end: payload bytes survive
    // a string round-trip, and the signing request travels with the
    // unsigned bytes so the client doesn't have to recompute the digest.
    let on_the_wire = serde_json::to_string(&bundle).expect("ser");
    let received: UnsignedBundle = serde_json::from_str(&on_the_wire).expect("de");
    assert_eq!(received, bundle, "wire roundtrip must be lossless");

    // ── Client side ────────────────────────────────────────────────
    // Holds: signer + a separate RPC provider for broadcast. No reader,
    // no fee estimator — those already ran on the server.
    let client_asserter = Asserter::new();
    let expected_hash = B256::repeat_byte(0xab);
    client_asserter.push_success(&expected_hash);
    let client_provider = ProviderBuilder::new()
        .disable_recommended_fillers()
        .connect_mocked_client(client_asserter);
    let client_service = EvmChainService::new(
        client_provider,
        NetworkId::from_str("eip155:1").unwrap(),
        1,
        true,
    );

    let UnsignedBundle {
        unsigned,
        signing_request,
    } = received;
    let response: SigningResponse = client_signer
        .sign(signing_request)
        .await
        .expect("client sign");

    let result = client_service
        .assemble_and_broadcast(unsigned.clone(), response)
        .await
        .expect("client broadcast");

    // Hash matches what the broadcast RPC returned.
    assert_eq!(result.tx_hash, format!("{:#x}", expected_hash));

    // The signed envelope was actually signed by the client signer —
    // recovering the sender from the broadcast bytes returns the
    // LocalKeySigner address. This proves the codec on the client side
    // composed correctly with what the server prepared.
    //
    // (We can't easily fish the raw bytes back out of the broadcaster
    // — they were consumed inside `assemble_and_broadcast` — so we
    // reassemble for verification purposes only.)
    let response2: SigningResponse = client_signer
        .sign(bundle.signing_request)
        .await
        .expect("re-sign for verification");
    let signed = client_service
        .codec
        .assemble_signed(unsigned, response2)
        .expect("assemble for verification");
    let envelope: TxEnvelope = TxEnvelope::decode_2718(&mut &signed.raw[..]).unwrap();
    let recovered: Address = envelope.recover_signer().unwrap();
    let expected_signer: Address = client_address.parse().unwrap();
    assert_eq!(recovered, expected_signer);
}
