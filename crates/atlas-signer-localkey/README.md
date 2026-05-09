# atlas-signer-localkey

In-process secp256k1 reference signer for the Atlas SDK. Implements [`atlas_core::signing::SignerProvider`](../atlas-core/src/signing.rs).

`#![forbid(unsafe_code)]`.

## Why a reference signer?

Atlas's signing surface is provider-neutral so MPC, Privy, hardware wallets, and ERC-4337 bundlers can all plug into the same `SignerProvider` trait. A local-key signer is the simplest possible implementation — useful for tests, scripts, smoke flows, and anywhere the operator already holds the key in memory. Production-grade custody belongs in MPC / hardware-wallet adapters that this crate models.

## Construction paths

```text
                ┌─────────────────────────────────────────────────┐
                │                                                 │
   raw bytes    │     ┌─────────────────────┐                     │
   [u8; 32] ──► │     │ from_bytes          │                     │
                │     └─────────────────────┘                     │
                │                                                 │
   Web3 keystore│     ┌─────────────────────┐                     │
   JSON ──────► │     │ from_keystore       │ (eth-keystore 0.5)  │
                │     │ from_keystore_path  │                     │
                │     └─────────────────────┘                     │
                │                                                 │
   BIP-39       │     ┌─────────────────────┐                     │
   mnemonic +   │     │ from_mnemonic       │ (bip39 + bip32)     │
   BIP-32 path  │ ──► └─────────────────────┘                     │
                │              │                                  │
                │              ▼                                  │
                │     ┌─────────────────────┐                     │
                │     │  LocalKeySigner     │                     │
                │     │   inner: alloy_signer_local::             │
                │     │          PrivateKeySigner                 │
                │     │   id:    SignerId                         │
                │     └─────────────────────┘                     │
                │              │                                  │
                │              ▼                                  │
                │     impl SignerProvider                         │
                │       async fn sign(req: SigningRequest)        │
                │            -> SigningResponse::SignatureOnly    │
                │              { signer, signature, public_key }  │
                │                                                 │
                └─────────────────────────────────────────────────┘
```

All three paths converge on the same internal `PrivateKeySigner` and produce raw 65-byte signatures (`r ‖ s ‖ v` with `v ∈ {0, 1}`). Chain-specific assembly — EIP-155 / EIP-1559 envelope encoding — is the codec's job, not the signer's. This is what makes the same crate sign for EVM today and for any future secp256k1-based chain (Bitcoin Taproot, Cosmos, etc.) tomorrow with no changes here.

## API

```rust
use atlas_core::id::SignerId;
use atlas_core::signing::SignerProvider;
use atlas_signer_localkey::LocalKeySigner;
use std::str::FromStr;

let id = SignerId::from_str("local-1")?;

// 1. From raw bytes
let signer = LocalKeySigner::from_bytes(id.clone(), [0x4c; 32])?;

// 2. From a Web3 secret-storage JSON keystore
let signer = LocalKeySigner::from_keystore(id.clone(), keystore_json, password)?;
let signer = LocalKeySigner::from_keystore_path(id.clone(), "key.json", password)?;

// 3. From a BIP-39 mnemonic + BIP-32 path
//    The path itself lives on Chain.default_derivation_path in the registry.
let signer = LocalKeySigner::from_mnemonic(
    id,
    "abandon abandon abandon … about",
    "m/44'/60'/0'/0/0",
)?;

println!("{}", signer.address());  // EIP-55-checksummed address
```

The HD derivation path is registry-driven — see [`Chain::default_derivation_path`](../atlas-core/src/chain.rs) and the [registries README](../../registries/README.md). Tests load the path from the official registry rather than hardcoding it.

## Curve + payload contract

| Property | Value |
|---|---|
| Curve | `Curve::Secp256k1` only — other curves return `SigningError::UnsupportedCurve` |
| Accepted payload | `SigningPayloadKind::TransactionDigest` (32-byte keccak digest) |
| Returned shape | `SigningResponse::SignatureOnly { signer, signature: [r ‖ s ‖ v], public_key: 0x04 ‖ X ‖ Y }` |

## Testing

```bash
cargo test -p atlas-signer-localkey
```

See [`tests/README.md`](tests/README.md) for the layout — raw-key roundtrips, Web3 keystore decryption, BIP-39 / BIP-32 derivation against a known address, and error paths.

## License

Licensed under the [MIT License](../../LICENSE).
