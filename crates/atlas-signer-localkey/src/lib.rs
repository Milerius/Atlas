//! In-process secp256k1 reference signer for the Atlas SDK.
//!
//! Three construction paths:
//! - [`LocalKeySigner::from_bytes`] — raw 32-byte private key.
//! - [`LocalKeySigner::from_keystore`] — Web3 secret-storage JSON keystore.
//! - [`LocalKeySigner::from_mnemonic`] — BIP-39 mnemonic + BIP-32 path.
//!
//! Implements [`atlas_core::signing::SignerProvider`]. Returns
//! [`SigningResponse::SignatureOnly`] (raw 65-byte signature + public key).
//! The chain codec handles assembly. The signer is **chain-agnostic** at the
//! signing step — same crate signs for EVM today and any future
//! secp256k1-based chain tomorrow.

#![forbid(unsafe_code)]

use alloy_primitives::B256;
use alloy_signer::SignerSync;
use alloy_signer_local::PrivateKeySigner;
use async_trait::async_trait;
use atlas_core::chain::Curve;
use atlas_core::error::SigningError;
use atlas_core::id::SignerId;
use atlas_core::signing::{SignerProvider, SigningPayloadKind, SigningRequest, SigningResponse};

pub struct LocalKeySigner {
    inner: PrivateKeySigner,
    id: SignerId,
}

impl LocalKeySigner {
    /// Construct from a raw 32-byte secp256k1 private key.
    pub fn from_bytes(id: SignerId, bytes: [u8; 32]) -> Result<Self, SigningError> {
        let inner = PrivateKeySigner::from_bytes(&bytes.into())
            .map_err(|e| SigningError::SignatureFailed(e.to_string()))?;
        Ok(Self { inner, id })
    }

    /// Construct from a Web3 secret-storage JSON keystore string.
    ///
    /// The eth-keystore crate's stable API at version 0.5.x is path-based;
    /// this method writes the JSON to a temp file and delegates. If you have
    /// the JSON already on disk, use [`Self::from_keystore_path`].
    pub fn from_keystore(id: SignerId, json: &str, password: &str) -> Result<Self, SigningError> {
        // eth-keystore 0.5 expects a path to the JSON file. Write the JSON
        // to a tempfile, read it back as a key, drop the tempfile.
        //
        // Tempfile creation / write failures only happen when the OS is in
        // a degraded state (full disk, no permissions to TMPDIR) — the
        // signer is not the layer that recovers from that, so we surface
        // such failures as panics rather than typed errors.
        use std::io::Write;
        let mut tmp = tempfile::NamedTempFile::new()
            .expect("tempfile creation should not fail in a healthy runtime");
        tmp.write_all(json.as_bytes())
            .expect("tempfile write should not fail in a healthy runtime");
        Self::from_keystore_path(id, tmp.path(), password)
    }

    /// Construct from a JSON keystore at `path`.
    pub fn from_keystore_path(
        id: SignerId,
        path: impl AsRef<std::path::Path>,
        password: &str,
    ) -> Result<Self, SigningError> {
        // eth-keystore 0.5 deserialises the keystore JSON into a typed
        // `EthKeystore` whose `crypto.ciphertext` is bounded to 32 bytes
        // for secp256k1, so the decrypted key is always exactly 32 bytes
        // when decryption succeeds. A length mismatch would be a library
        // bug, not a runtime input we can recover from.
        let key_bytes: [u8; 32] = eth_keystore::decrypt_key(path.as_ref(), password)
            .map_err(|e| SigningError::SignatureFailed(format!("keystore decrypt: {e}")))?
            .try_into()
            .expect("eth-keystore 0.5 returns exactly 32 bytes for secp256k1");
        Self::from_bytes(id, key_bytes)
    }

    /// Construct from a BIP-39 mnemonic + BIP-32 derivation path.
    pub fn from_mnemonic(
        id: SignerId,
        mnemonic: &str,
        derivation_path: &str,
    ) -> Result<Self, SigningError> {
        use bip32::DerivationPath;
        use bip39::{Language, Mnemonic};
        use std::str::FromStr;

        // The mnemonic and derivation path strings are user-supplied and
        // surface typed errors. The derivation arithmetic that follows is
        // total: a 64-byte BIP-39 seed is always a valid BIP-32 master
        // input, and child-derivation never fails on a syntactically
        // parsed `DerivationPath` — those would be library bugs we have
        // no graceful recovery for.
        let mnemonic = Mnemonic::parse_in(Language::English, mnemonic)
            .map_err(|e| SigningError::SignatureFailed(e.to_string()))?;
        let path = DerivationPath::from_str(derivation_path)
            .map_err(|e| SigningError::SignatureFailed(e.to_string()))?;
        let seed = mnemonic.to_seed("");
        let xprv = bip32::XPrv::new(seed).expect("BIP-32 master from a 64-byte seed is total");
        let mut child = xprv;
        for n in path.iter() {
            child = child
                .derive_child(n)
                .expect("BIP-32 child derivation is total on a parsed DerivationPath");
        }
        // bip32 0.5/0.6: to_bytes() returns [u8; 32] (PrivateKeyBytes).
        let private_bytes: [u8; 32] = child.to_bytes();
        Self::from_bytes(id, private_bytes)
    }

    /// EIP-55-checksummed Ethereum address derived from this key.
    ///
    /// Uses alloy's `Display` impl on `Address`, which emits the canonical
    /// EIP-55 mixed-case checksum form (`{:x}` would emit lowercase only).
    pub fn address(&self) -> String {
        format!("{}", self.inner.address())
    }
}

#[async_trait]
impl SignerProvider for LocalKeySigner {
    fn id(&self) -> &SignerId {
        &self.id
    }

    async fn sign(&self, request: SigningRequest) -> Result<SigningResponse, SigningError> {
        if !matches!(request.curve, Curve::Secp256k1) {
            return Err(SigningError::UnsupportedCurve(format!(
                "{:?}",
                request.curve
            )));
        }

        let signature = match request.payload_kind {
            SigningPayloadKind::TransactionDigest => {
                if request.payload.len() != 32 {
                    return Err(SigningError::UnsupportedPayload(format!(
                        "TransactionDigest expected 32 bytes, got {}",
                        request.payload.len()
                    )));
                }
                let digest = B256::from_slice(&request.payload);
                // Signing a 32-byte digest with a constructed
                // `PrivateKeySigner` is total: the key was validated at
                // construction (`from_bytes`) and the digest length was
                // checked above, so alloy's `sign_hash_sync` cannot
                // surface an error on this path.
                self.inner
                    .sign_hash_sync(&digest)
                    .expect("sign_hash_sync is total for a valid key + 32-byte digest")
            }
            other => {
                return Err(SigningError::UnsupportedPayload(format!(
                    "{:?} not supported by LocalKeySigner v1",
                    other
                )));
            }
        };

        let mut sig_bytes = Vec::with_capacity(65);
        sig_bytes.extend_from_slice(&signature.r().to_be_bytes::<32>());
        sig_bytes.extend_from_slice(&signature.s().to_be_bytes::<32>());
        sig_bytes.push(if signature.v() { 1 } else { 0 });

        // Public key: uncompressed 65-byte form (0x04 || X || Y).
        // PrivateKeySigner::public_key() returns B512 (64 bytes, X||Y without 0x04).
        let raw_pubkey = self.inner.public_key();
        let mut pubkey_bytes = Vec::with_capacity(65);
        pubkey_bytes.push(0x04u8);
        pubkey_bytes.extend_from_slice(raw_pubkey.as_slice());

        Ok(SigningResponse::SignatureOnly {
            signer: self.id.clone(),
            signature: sig_bytes,
            public_key: pubkey_bytes,
        })
    }
}
