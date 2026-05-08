# Atlas Core Blockchain Layer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the first compileable Atlas Rust workspace with precise domain models, split registries, validation, exact-asset execution boundaries, provider-neutral signing traits, and a mocked EVM transaction flow.

**Architecture:** Start with a strict blockchain core. `atlas-core` owns shared ids, precision types, errors, assets, registries, signing requests, transaction intents, and chain-service traits. EVM implementation remains minimal and mock-backed in this milestone; public RPC, Wallet Core integration, generated bindings, and concrete signer providers come later.

**Tech Stack:** Rust 2021, Cargo workspace, `serde`, `serde_json`, `thiserror`, `num-bigint`, `rust_decimal`, `async-trait`, `tokio` for async tests.

---

## Related Documents

- Core design spec: `docs/superpowers/specs/2026-05-08-atlas-core-blockchain-layer-design.md`
- Product account composition notes: `docs/superpowers/specs/2026-05-08-atlas-product-accounts-composition-notes.md`
- Repository-wide rules: `CODEX.md`

## File Structure

- `Cargo.toml`: workspace manifest and shared dependencies.
- `crates/atlas-core/Cargo.toml`: core crate manifest.
- `crates/atlas-core/src/lib.rs`: module exports.
- `crates/atlas-core/src/id.rs`: typed ids and parser helpers.
- `crates/atlas-core/src/amount.rs`: `RawAmount` and decimal-safe value helpers.
- `crates/atlas-core/src/error.rs`: typed error enums.
- `crates/atlas-core/src/asset.rs`: asset taxonomy and asset registry models.
- `crates/atlas-core/src/chain.rs`: chain and network registry models.
- `crates/atlas-core/src/registry.rs`: registry documents, layered registry, validation, and lookups.
- `crates/atlas-core/src/signing.rs`: signer refs, signing requests, signing responses, and signer trait.
- `crates/atlas-core/src/transaction.rs`: transfer intent, fee quote, unsigned/signed transaction models.
- `crates/atlas-core/src/service.rs`: `ChainService` trait and `MockEvmService` for boundary tests.
- `crates/atlas-core/tests/fixtures/*.json`: valid and invalid registry fixtures.
- `crates/atlas-core/tests/registry_validation.rs`: fixture validation tests.
- `crates/atlas-core/tests/asset_resolution.rs`: USDC/ETH resolution tests.
- `crates/atlas-core/tests/smoke_flow.rs`: registry -> chain service -> mock signer -> broadcast smoke test.

## Task 1: Workspace And Core Crate

**Files:**
- Create: `Cargo.toml`
- Create: `crates/atlas-core/Cargo.toml`
- Create: `crates/atlas-core/src/lib.rs`

- [ ] **Step 1: Create the workspace manifest**

Write `Cargo.toml`:

```toml
[workspace]
members = ["crates/atlas-core"]
resolver = "2"

[workspace.package]
edition = "2021"
license = "Apache-2.0"
repository = "https://github.com/milerius/Atlas"

[workspace.dependencies]
async-trait = "0.1"
num-bigint = { version = "0.4", features = ["serde"] }
num-traits = "0.2"
rust_decimal = { version = "1", features = ["serde"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

- [ ] **Step 2: Create the core crate manifest**

Write `crates/atlas-core/Cargo.toml`:

```toml
[package]
name = "atlas-core"
version = "0.1.0"
edition.workspace = true
license.workspace = true
repository.workspace = true

[dependencies]
async-trait.workspace = true
num-bigint.workspace = true
num-traits.workspace = true
rust_decimal.workspace = true
serde.workspace = true
thiserror.workspace = true

[dev-dependencies]
serde_json.workspace = true
tokio.workspace = true
```

- [ ] **Step 3: Create module exports**

Write `crates/atlas-core/src/lib.rs`:

```rust
#![forbid(unsafe_code)]

pub mod amount;
pub mod asset;
pub mod chain;
pub mod error;
pub mod id;
pub mod registry;
pub mod service;
pub mod signing;
pub mod transaction;

pub use amount::RawAmount;
pub use error::{AssetError, ChainError, RegistryError, RpcError, SigningError};
```

- [ ] **Step 4: Run the initial crate check**

Run: `cargo check -p atlas-core`

Expected: FAIL with missing module file errors for `amount`, `asset`, `chain`, `error`, `id`, `registry`, `service`, `signing`, and `transaction`.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml crates/atlas-core/Cargo.toml crates/atlas-core/src/lib.rs
git commit -m "chore: scaffold atlas core workspace"
```

## Task 2: Typed IDs, Amounts, And Errors

**Files:**
- Create: `crates/atlas-core/src/id.rs`
- Create: `crates/atlas-core/src/amount.rs`
- Create: `crates/atlas-core/src/error.rs`

- [ ] **Step 1: Write typed id tests in `id.rs`**

Create `crates/atlas-core/src/id.rs` with the tests first:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn network_id_rejects_empty() {
        assert_eq!(
            NetworkId::new("").unwrap_err().to_string(),
            "id must not be empty"
        );
    }

    #[test]
    fn asset_instance_id_preserves_value() {
        let id = AssetInstanceId::new("eip155:8453/native:eth").unwrap();
        assert_eq!(id.as_str(), "eip155:8453/native:eth");
    }
}
```

- [ ] **Step 2: Run id tests to verify they fail**

Run: `cargo test -p atlas-core id::tests -- --nocapture`

Expected: FAIL because `NetworkId` and `AssetInstanceId` are not defined.

- [ ] **Step 3: Implement typed ids**

Replace `crates/atlas-core/src/id.rs` with:

```rust
use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Id(String);

impl Id {
    pub fn new(value: impl Into<String>) -> Result<Self, IdError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(IdError::Empty);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for Id {
    type Err = IdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum IdError {
    #[error("id must not be empty")]
    Empty,
}

macro_rules! typed_id {
    ($name:ident) => {
        #[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(Id);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, IdError> {
                Id::new(value).map(Self)
            }

            pub fn as_str(&self) -> &str {
                self.0.as_str()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }

        impl FromStr for $name {
            type Err = IdError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::new(value)
            }
        }
    };
}

typed_id!(ChainId);
typed_id!(NetworkId);
typed_id!(AssetGroupId);
typed_id!(AssetInstrumentId);
typed_id!(AssetInstanceId);
typed_id!(SignerId);
typed_id!(AccountRef);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn network_id_rejects_empty() {
        assert_eq!(
            NetworkId::new("").unwrap_err().to_string(),
            "id must not be empty"
        );
    }

    #[test]
    fn asset_instance_id_preserves_value() {
        let id = AssetInstanceId::new("eip155:8453/native:eth").unwrap();
        assert_eq!(id.as_str(), "eip155:8453/native:eth");
    }
}
```

- [ ] **Step 4: Write precision tests in `amount.rs`**

Create `crates/atlas-core/src/amount.rs` with:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use num_bigint::BigInt;

    #[test]
    fn raw_amount_preserves_large_integer() {
        let amount = RawAmount::new(BigInt::parse_bytes(b"1000000000000000000000000000000", 10).unwrap(), 18);
        assert_eq!(amount.decimals(), 18);
        assert_eq!(amount.value().to_string(), "1000000000000000000000000000000");
    }

    #[test]
    fn checked_add_rejects_decimal_mismatch() {
        let a = RawAmount::new(BigInt::from(1), 6);
        let b = RawAmount::new(BigInt::from(1), 18);
        assert_eq!(
            a.checked_add(&b).unwrap_err().to_string(),
            "amount decimals mismatch: left=6 right=18"
        );
    }
}
```

- [ ] **Step 5: Run amount tests to verify they fail**

Run: `cargo test -p atlas-core amount::tests -- --nocapture`

Expected: FAIL because `RawAmount` is not defined.

- [ ] **Step 6: Implement `RawAmount`**

Replace `crates/atlas-core/src/amount.rs` with:

```rust
use num_bigint::BigInt;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RawAmount {
    value: BigInt,
    decimals: u8,
}

impl RawAmount {
    pub fn new(value: BigInt, decimals: u8) -> Self {
        Self { value, decimals }
    }

    pub fn value(&self) -> &BigInt {
        &self.value
    }

    pub fn decimals(&self) -> u8 {
        self.decimals
    }

    pub fn checked_add(&self, rhs: &Self) -> Result<Self, AmountError> {
        if self.decimals != rhs.decimals {
            return Err(AmountError::DecimalsMismatch {
                left: self.decimals,
                right: rhs.decimals,
            });
        }
        Ok(Self::new(&self.value + &rhs.value, self.decimals))
    }
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum AmountError {
    #[error("amount decimals mismatch: left={left} right={right}")]
    DecimalsMismatch { left: u8, right: u8 },
}

#[cfg(test)]
mod tests {
    use super::*;
    use num_bigint::BigInt;

    #[test]
    fn raw_amount_preserves_large_integer() {
        let amount = RawAmount::new(BigInt::parse_bytes(b"1000000000000000000000000000000", 10).unwrap(), 18);
        assert_eq!(amount.decimals(), 18);
        assert_eq!(amount.value().to_string(), "1000000000000000000000000000000");
    }

    #[test]
    fn checked_add_rejects_decimal_mismatch() {
        let a = RawAmount::new(BigInt::from(1), 6);
        let b = RawAmount::new(BigInt::from(1), 18);
        assert_eq!(
            a.checked_add(&b).unwrap_err().to_string(),
            "amount decimals mismatch: left=6 right=18"
        );
    }
}
```

- [ ] **Step 7: Implement typed errors**

Write `crates/atlas-core/src/error.rs`:

```rust
use crate::id::{AssetGroupId, AssetInstanceId, AssetInstrumentId, ChainId, NetworkId, SignerId};

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum RegistryError {
    #[error("missing chain: {0}")]
    MissingChain(ChainId),
    #[error("missing network: {0}")]
    MissingNetwork(NetworkId),
    #[error("missing asset group: {0}")]
    MissingAssetGroup(AssetGroupId),
    #[error("missing asset instrument: {0}")]
    MissingAssetInstrument(AssetInstrumentId),
    #[error("missing asset instance: {0}")]
    MissingAssetInstance(AssetInstanceId),
    #[error("invalid registry reference: {message}")]
    InvalidReference { message: String },
    #[error("unsupported registry version: {version}")]
    UnsupportedVersion { version: u32 },
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum AssetError {
    #[error("unsupported asset standard: {0}")]
    UnsupportedStandard(String),
    #[error("invalid contract: {0}")]
    InvalidContract(String),
    #[error("invalid decimals: {0}")]
    InvalidDecimals(u8),
    #[error("missing required identifier: {0}")]
    MissingRequiredIdentifier(String),
    #[error("asset is not executable: {0}")]
    AssetNotExecutable(String),
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ChainError {
    #[error("unsupported chain: {0}")]
    UnsupportedChain(ChainId),
    #[error("unsupported network: {0}")]
    UnsupportedNetwork(NetworkId),
    #[error("unsupported asset instance: {0}")]
    UnsupportedAssetInstance(AssetInstanceId),
    #[error("invalid address: {0}")]
    InvalidAddress(String),
    #[error("fee estimation failed: {0}")]
    FeeEstimationFailed(String),
    #[error("transaction build failed: {0}")]
    TransactionBuildFailed(String),
    #[error("broadcast failed: {0}")]
    BroadcastFailed(String),
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum SigningError {
    #[error("signer not found: {0}")]
    SignerNotFound(SignerId),
    #[error("unsupported curve: {0}")]
    UnsupportedCurve(String),
    #[error("unsupported payload: {0}")]
    UnsupportedPayload(String),
    #[error("user rejected signing")]
    UserRejected,
    #[error("signature failed: {0}")]
    SignatureFailed(String),
    #[error("invalid signature: {0}")]
    InvalidSignature(String),
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum RpcError {
    #[error("transport error: {0}")]
    Transport(String),
    #[error("request timed out")]
    Timeout,
    #[error("rate limited")]
    RateLimited,
    #[error("malformed response: {0}")]
    MalformedResponse(String),
    #[error("node error: {0}")]
    NodeError(String),
}
```

- [ ] **Step 8: Run tests**

Run: `cargo test -p atlas-core id::tests amount::tests -- --nocapture`

Expected: PASS for 4 tests.

- [ ] **Step 9: Run crate check**

Run: `cargo check -p atlas-core`

Expected: FAIL with missing module file errors for `asset`, `chain`, `registry`, `service`, `signing`, and `transaction`.

- [ ] **Step 10: Commit**

```bash
git add crates/atlas-core/src/id.rs crates/atlas-core/src/amount.rs crates/atlas-core/src/error.rs
git commit -m "feat(core): add ids amounts and typed errors"
```

## Task 3: Asset And Chain Models

**Files:**
- Create: `crates/atlas-core/src/asset.rs`
- Create: `crates/atlas-core/src/chain.rs`

- [ ] **Step 1: Write asset model tests**

Create `crates/atlas-core/src/asset.rs` with:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::{AssetGroupId, AssetInstrumentId, AssetInstanceId, NetworkId};
    use std::str::FromStr;

    #[test]
    fn erc20_instance_requires_contract() {
        let instance = AssetInstance {
            id: AssetInstanceId::from_str("eip155:8453/erc20:missing").unwrap(),
            instrument_id: AssetInstrumentId::from_str("usdc.circle").unwrap(),
            network: NetworkId::from_str("eip155:8453").unwrap(),
            standard: AssetStandard::Erc20,
            decimals: 6,
            contract: None,
            capabilities: vec![AssetCapability::Balance],
            metadata: AssetMetadata::default(),
        };

        assert_eq!(
            instance.validate_shape().unwrap_err().to_string(),
            "missing required identifier: erc20 contract"
        );
    }

    #[test]
    fn native_instance_rejects_contract() {
        let instance = AssetInstance {
            id: AssetInstanceId::from_str("eip155:1/native:eth").unwrap(),
            instrument_id: AssetInstrumentId::from_str("eth.native").unwrap(),
            network: NetworkId::from_str("eip155:1").unwrap(),
            standard: AssetStandard::Native,
            decimals: 18,
            contract: Some("0xabc".to_string()),
            capabilities: vec![AssetCapability::Balance],
            metadata: AssetMetadata::default(),
        };

        assert_eq!(
            instance.validate_shape().unwrap_err().to_string(),
            "invalid contract: native asset must not have a contract"
        );
    }

    #[test]
    fn asset_group_is_display_only() {
        let group = AssetGroup {
            id: AssetGroupId::from_str("usdc").unwrap(),
            symbol: "USDC".to_string(),
            name: "USDC".to_string(),
            metadata: AssetMetadata::default(),
        };

        assert_eq!(group.symbol, "USDC");
    }
}
```

- [ ] **Step 2: Run asset tests to verify they fail**

Run: `cargo test -p atlas-core asset::tests -- --nocapture`

Expected: FAIL because asset model types are not defined.

- [ ] **Step 3: Implement asset models**

Replace `crates/atlas-core/src/asset.rs` with:

```rust
use crate::{
    error::AssetError,
    id::{AssetGroupId, AssetInstanceId, AssetInstrumentId, NetworkId},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AssetGroup {
    pub id: AssetGroupId,
    pub symbol: String,
    pub name: String,
    #[serde(default)]
    pub metadata: AssetMetadata,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AssetInstrument {
    pub id: AssetInstrumentId,
    #[serde(rename = "groupId")]
    pub group_id: AssetGroupId,
    #[serde(rename = "assetClass")]
    pub asset_class: AssetClass,
    pub kind: InstrumentKind,
    pub symbol: String,
    pub name: String,
    pub decimals: u8,
    #[serde(default)]
    pub issuer: Option<String>,
    #[serde(default)]
    pub traits: Vec<AssetTrait>,
    #[serde(default)]
    pub metadata: AssetMetadata,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AssetInstance {
    pub id: AssetInstanceId,
    #[serde(rename = "instrumentId")]
    pub instrument_id: AssetInstrumentId,
    pub network: NetworkId,
    pub standard: AssetStandard,
    pub decimals: u8,
    #[serde(default)]
    pub contract: Option<String>,
    #[serde(default)]
    pub capabilities: Vec<AssetCapability>,
    #[serde(default)]
    pub metadata: AssetMetadata,
}

impl AssetInstance {
    pub fn validate_shape(&self) -> Result<(), AssetError> {
        match (&self.standard, self.contract.as_deref()) {
            (AssetStandard::Native, None) => Ok(()),
            (AssetStandard::Native, Some(_)) => {
                Err(AssetError::InvalidContract("native asset must not have a contract".to_string()))
            }
            (AssetStandard::Erc20, Some(value)) if !value.trim().is_empty() => Ok(()),
            (AssetStandard::Erc20, _) => Err(AssetError::MissingRequiredIdentifier("erc20 contract".to_string())),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetClass {
    Crypto,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstrumentKind {
    NativeCoin,
    FungibleToken,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetStandard {
    Native,
    Erc20,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetTrait {
    Fungible,
    Transferable,
    GasAsset,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetCapability {
    Balance,
    Transfer,
    Approve,
    PayGas,
    Swap,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct AssetMetadata {
    #[serde(flatten)]
    pub values: BTreeMap<String, String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::{AssetGroupId, AssetInstrumentId, AssetInstanceId, NetworkId};
    use std::str::FromStr;

    #[test]
    fn erc20_instance_requires_contract() {
        let instance = AssetInstance {
            id: AssetInstanceId::from_str("eip155:8453/erc20:missing").unwrap(),
            instrument_id: AssetInstrumentId::from_str("usdc.circle").unwrap(),
            network: NetworkId::from_str("eip155:8453").unwrap(),
            standard: AssetStandard::Erc20,
            decimals: 6,
            contract: None,
            capabilities: vec![AssetCapability::Balance],
            metadata: AssetMetadata::default(),
        };

        assert_eq!(
            instance.validate_shape().unwrap_err().to_string(),
            "missing required identifier: erc20 contract"
        );
    }

    #[test]
    fn native_instance_rejects_contract() {
        let instance = AssetInstance {
            id: AssetInstanceId::from_str("eip155:1/native:eth").unwrap(),
            instrument_id: AssetInstrumentId::from_str("eth.native").unwrap(),
            network: NetworkId::from_str("eip155:1").unwrap(),
            standard: AssetStandard::Native,
            decimals: 18,
            contract: Some("0xabc".to_string()),
            capabilities: vec![AssetCapability::Balance],
            metadata: AssetMetadata::default(),
        };

        assert_eq!(
            instance.validate_shape().unwrap_err().to_string(),
            "invalid contract: native asset must not have a contract"
        );
    }

    #[test]
    fn asset_group_is_display_only() {
        let group = AssetGroup {
            id: AssetGroupId::from_str("usdc").unwrap(),
            symbol: "USDC".to_string(),
            name: "USDC".to_string(),
            metadata: AssetMetadata::default(),
        };

        assert_eq!(group.symbol, "USDC");
    }
}
```

- [ ] **Step 4: Write chain model tests**

Create `crates/atlas-core/src/chain.rs` with:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::{AssetInstanceId, ChainId, NetworkId};
    use std::str::FromStr;

    #[test]
    fn network_references_chain_and_native_asset() {
        let network = Network {
            id: NetworkId::from_str("eip155:8453").unwrap(),
            alias: Some("base".to_string()),
            chain_id: Some("8453".to_string()),
            chain: ChainId::from_str("evm").unwrap(),
            name: "Base".to_string(),
            environment: NetworkEnvironment::Mainnet,
            native_asset_instance_id: AssetInstanceId::from_str("eip155:8453/native:eth").unwrap(),
            rpc: RpcConfig { default_url: "https://mainnet.base.org".to_string() },
            explorers: vec![],
            features: NetworkFeatures::default(),
        };

        assert_eq!(network.chain.as_str(), "evm");
    }
}
```

- [ ] **Step 5: Run chain tests to verify they fail**

Run: `cargo test -p atlas-core chain::tests -- --nocapture`

Expected: FAIL because chain model types are not defined.

- [ ] **Step 6: Implement chain models**

Replace `crates/atlas-core/src/chain.rs` with:

```rust
use crate::id::{AssetInstanceId, ChainId, NetworkId};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Chain {
    pub id: ChainId,
    pub name: String,
    pub family: ChainFamily,
    #[serde(rename = "addressFormat")]
    pub address_format: AddressFormat,
    #[serde(rename = "defaultCurve")]
    pub default_curve: Curve,
    #[serde(rename = "supportedStandards")]
    pub supported_standards: Vec<String>,
    #[serde(default)]
    pub capabilities: Vec<ChainCapability>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Network {
    pub id: NetworkId,
    #[serde(default)]
    pub alias: Option<String>,
    #[serde(default, rename = "chainId")]
    pub chain_id: Option<String>,
    pub chain: ChainId,
    pub name: String,
    pub environment: NetworkEnvironment,
    #[serde(rename = "nativeAssetInstanceId")]
    pub native_asset_instance_id: AssetInstanceId,
    pub rpc: RpcConfig,
    #[serde(default)]
    pub explorers: Vec<Explorer>,
    #[serde(default)]
    pub features: NetworkFeatures,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChainFamily {
    AccountBased,
    Utxo,
    ObjectBased,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddressFormat {
    EvmAddress,
    SolanaPubkey,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Curve {
    Secp256k1,
    Ed25519,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChainCapability {
    Balance,
    Transfer,
    Approve,
    ContractCall,
    Broadcast,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkEnvironment {
    Mainnet,
    Testnet,
    Devnet,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RpcConfig {
    #[serde(rename = "defaultUrl")]
    pub default_url: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Explorer {
    pub name: String,
    pub tx: String,
    pub address: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct NetworkFeatures {
    #[serde(default)]
    pub eip1559: bool,
    #[serde(default)]
    pub erc20: bool,
    #[serde(default, rename = "opStackL1Fee")]
    pub op_stack_l1_fee: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::{AssetInstanceId, ChainId, NetworkId};
    use std::str::FromStr;

    #[test]
    fn network_references_chain_and_native_asset() {
        let network = Network {
            id: NetworkId::from_str("eip155:8453").unwrap(),
            alias: Some("base".to_string()),
            chain_id: Some("8453".to_string()),
            chain: ChainId::from_str("evm").unwrap(),
            name: "Base".to_string(),
            environment: NetworkEnvironment::Mainnet,
            native_asset_instance_id: AssetInstanceId::from_str("eip155:8453/native:eth").unwrap(),
            rpc: RpcConfig { default_url: "https://mainnet.base.org".to_string() },
            explorers: vec![],
            features: NetworkFeatures::default(),
        };

        assert_eq!(network.chain.as_str(), "evm");
    }
}
```

- [ ] **Step 7: Run model tests**

Run: `cargo test -p atlas-core asset::tests chain::tests -- --nocapture`

Expected: PASS for 4 tests.

- [ ] **Step 8: Run crate check**

Run: `cargo check -p atlas-core`

Expected: FAIL with missing module file errors for `registry`, `service`, `signing`, and `transaction`.

- [ ] **Step 9: Commit**

```bash
git add crates/atlas-core/src/asset.rs crates/atlas-core/src/chain.rs
git commit -m "feat(core): model chains networks and assets"
```

## Task 4: Registry Documents And Validation

**Files:**
- Create: `crates/atlas-core/src/registry.rs`
- Create: `crates/atlas-core/tests/fixtures/chain_registry.valid.json`
- Create: `crates/atlas-core/tests/fixtures/asset_registry.valid.json`
- Create: `crates/atlas-core/tests/fixtures/asset_registry.invalid_missing_network.json`
- Create: `crates/atlas-core/tests/fixtures/asset_registry.invalid_native_contract.json`
- Create: `crates/atlas-core/tests/registry_validation.rs`

- [ ] **Step 1: Create valid chain registry fixture**

Write `crates/atlas-core/tests/fixtures/chain_registry.valid.json`:

```json
{
  "version": 1,
  "chains": [
    {
      "id": "evm",
      "name": "EVM",
      "family": "account_based",
      "addressFormat": "evm_address",
      "defaultCurve": "secp256k1",
      "supportedStandards": ["native", "erc20"],
      "capabilities": ["balance", "transfer", "approve", "contract_call", "broadcast"]
    }
  ],
  "networks": [
    {
      "id": "eip155:1",
      "alias": "ethereum",
      "chainId": "1",
      "chain": "evm",
      "name": "Ethereum",
      "environment": "mainnet",
      "nativeAssetInstanceId": "eip155:1/native:eth",
      "rpc": { "defaultUrl": "https://ethereum-rpc.publicnode.com" },
      "explorers": [],
      "features": { "eip1559": true, "erc20": true }
    },
    {
      "id": "eip155:8453",
      "alias": "base",
      "chainId": "8453",
      "chain": "evm",
      "name": "Base",
      "environment": "mainnet",
      "nativeAssetInstanceId": "eip155:8453/native:eth",
      "rpc": { "defaultUrl": "https://mainnet.base.org" },
      "explorers": [],
      "features": { "eip1559": true, "erc20": true, "opStackL1Fee": true }
    }
  ]
}
```

- [ ] **Step 2: Create valid asset registry fixture**

Write `crates/atlas-core/tests/fixtures/asset_registry.valid.json`:

```json
{
  "version": 1,
  "assetGroups": [
    { "id": "eth", "symbol": "ETH", "name": "Ethereum" },
    { "id": "usdc", "symbol": "USDC", "name": "USDC" }
  ],
  "assetInstruments": [
    {
      "id": "eth.native",
      "groupId": "eth",
      "assetClass": "crypto",
      "kind": "native_coin",
      "symbol": "ETH",
      "name": "Ether",
      "decimals": 18,
      "traits": ["fungible", "transferable", "gas_asset"]
    },
    {
      "id": "usdc.circle",
      "groupId": "usdc",
      "assetClass": "crypto",
      "kind": "fungible_token",
      "symbol": "USDC",
      "name": "USD Coin",
      "decimals": 6,
      "issuer": "Circle",
      "traits": ["fungible", "transferable"]
    }
  ],
  "assetInstances": [
    {
      "id": "eip155:1/native:eth",
      "instrumentId": "eth.native",
      "network": "eip155:1",
      "standard": "native",
      "decimals": 18,
      "capabilities": ["balance", "transfer", "pay_gas"]
    },
    {
      "id": "eip155:8453/native:eth",
      "instrumentId": "eth.native",
      "network": "eip155:8453",
      "standard": "native",
      "decimals": 18,
      "capabilities": ["balance", "transfer", "pay_gas"]
    },
    {
      "id": "eip155:1/erc20:0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48",
      "instrumentId": "usdc.circle",
      "network": "eip155:1",
      "standard": "erc20",
      "decimals": 6,
      "contract": "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48",
      "capabilities": ["balance", "transfer", "approve", "swap"]
    },
    {
      "id": "eip155:8453/erc20:0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913",
      "instrumentId": "usdc.circle",
      "network": "eip155:8453",
      "standard": "erc20",
      "decimals": 6,
      "contract": "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913",
      "capabilities": ["balance", "transfer", "approve", "swap"]
    }
  ]
}
```

- [ ] **Step 3: Create invalid fixtures**

Write `crates/atlas-core/tests/fixtures/asset_registry.invalid_missing_network.json`:

```json
{
  "version": 1,
  "assetGroups": [{ "id": "bad", "symbol": "BAD", "name": "Bad Token" }],
  "assetInstruments": [
    {
      "id": "bad.token",
      "groupId": "bad",
      "assetClass": "crypto",
      "kind": "fungible_token",
      "symbol": "BAD",
      "name": "Bad Token",
      "decimals": 18,
      "traits": ["fungible", "transferable"]
    }
  ],
  "assetInstances": [
    {
      "id": "eip155:999/erc20:0xBAD",
      "instrumentId": "bad.token",
      "network": "eip155:999",
      "standard": "erc20",
      "decimals": 18,
      "contract": "0xBAD",
      "capabilities": ["balance"]
    }
  ]
}
```

Write `crates/atlas-core/tests/fixtures/asset_registry.invalid_native_contract.json`:

```json
{
  "version": 1,
  "assetGroups": [{ "id": "eth", "symbol": "ETH", "name": "Ethereum" }],
  "assetInstruments": [
    {
      "id": "eth.native",
      "groupId": "eth",
      "assetClass": "crypto",
      "kind": "native_coin",
      "symbol": "ETH",
      "name": "Ether",
      "decimals": 18,
      "traits": ["fungible", "transferable", "gas_asset"]
    }
  ],
  "assetInstances": [
    {
      "id": "eip155:1/native:eth",
      "instrumentId": "eth.native",
      "network": "eip155:1",
      "standard": "native",
      "decimals": 18,
      "contract": "0xabc",
      "capabilities": ["balance"]
    }
  ]
}
```

- [ ] **Step 4: Write failing registry validation tests**

Write `crates/atlas-core/tests/registry_validation.rs`:

```rust
use atlas_core::registry::{AssetRegistryDocument, ChainRegistryDocument, Registry};

fn parse_chain_registry() -> ChainRegistryDocument {
    serde_json::from_str(include_str!("fixtures/chain_registry.valid.json")).unwrap()
}

fn parse_asset_registry(path: &str) -> AssetRegistryDocument {
    serde_json::from_str(match path {
        "valid" => include_str!("fixtures/asset_registry.valid.json"),
        "missing_network" => include_str!("fixtures/asset_registry.invalid_missing_network.json"),
        "native_contract" => include_str!("fixtures/asset_registry.invalid_native_contract.json"),
        _ => panic!("unknown fixture key"),
    })
    .unwrap()
}

#[test]
fn valid_registries_load_and_validate() {
    let registry = Registry::from_documents(parse_chain_registry(), parse_asset_registry("valid")).unwrap();
    assert_eq!(registry.network("eip155:8453").unwrap().name, "Base");
    assert_eq!(registry.asset_group("usdc").unwrap().symbol, "USDC");
}

#[test]
fn asset_instance_with_missing_network_fails_validation() {
    let err = Registry::from_documents(parse_chain_registry(), parse_asset_registry("missing_network")).unwrap_err();
    assert_eq!(err.to_string(), "missing network: eip155:999");
}

#[test]
fn native_asset_with_contract_fails_validation() {
    let err = Registry::from_documents(parse_chain_registry(), parse_asset_registry("native_contract")).unwrap_err();
    assert_eq!(
        err.to_string(),
        "invalid registry reference: asset instance eip155:1/native:eth shape invalid: invalid contract: native asset must not have a contract"
    );
}
```

- [ ] **Step 5: Run registry tests to verify they fail**

Run: `cargo test -p atlas-core --test registry_validation -- --nocapture`

Expected: FAIL because `registry` module types are not defined.

- [ ] **Step 6: Implement registry parsing and validation**

Write `crates/atlas-core/src/registry.rs`:

```rust
use crate::{
    asset::{AssetGroup, AssetInstance, AssetInstrument},
    chain::{Chain, Network},
    error::RegistryError,
    id::{AssetGroupId, AssetInstanceId, AssetInstrumentId, NetworkId},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const LATEST_REGISTRY_VERSION: u32 = 1;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ChainRegistryDocument {
    pub version: u32,
    pub chains: Vec<Chain>,
    pub networks: Vec<Network>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AssetRegistryDocument {
    pub version: u32,
    #[serde(rename = "assetGroups")]
    pub asset_groups: Vec<AssetGroup>,
    #[serde(rename = "assetInstruments")]
    pub asset_instruments: Vec<AssetInstrument>,
    #[serde(rename = "assetInstances")]
    pub asset_instances: Vec<AssetInstance>,
}

#[derive(Clone, Debug)]
pub struct Registry {
    chains: BTreeMap<String, Chain>,
    networks: BTreeMap<String, Network>,
    asset_groups: BTreeMap<String, AssetGroup>,
    asset_instruments: BTreeMap<String, AssetInstrument>,
    asset_instances: BTreeMap<String, AssetInstance>,
}

impl Registry {
    pub fn from_documents(
        chain_doc: ChainRegistryDocument,
        asset_doc: AssetRegistryDocument,
    ) -> Result<Self, RegistryError> {
        reject_unknown_version(chain_doc.version)?;
        reject_unknown_version(asset_doc.version)?;

        let chains = chain_doc
            .chains
            .into_iter()
            .map(|chain| (chain.id.to_string(), chain))
            .collect::<BTreeMap<_, _>>();
        let networks = chain_doc
            .networks
            .into_iter()
            .map(|network| (network.id.to_string(), network))
            .collect::<BTreeMap<_, _>>();
        let asset_groups = asset_doc
            .asset_groups
            .into_iter()
            .map(|group| (group.id.to_string(), group))
            .collect::<BTreeMap<_, _>>();
        let asset_instruments = asset_doc
            .asset_instruments
            .into_iter()
            .map(|instrument| (instrument.id.to_string(), instrument))
            .collect::<BTreeMap<_, _>>();
        let asset_instances = asset_doc
            .asset_instances
            .into_iter()
            .map(|instance| (instance.id.to_string(), instance))
            .collect::<BTreeMap<_, _>>();

        let registry = Self {
            chains,
            networks,
            asset_groups,
            asset_instruments,
            asset_instances,
        };
        registry.validate_references()?;
        Ok(registry)
    }

    pub fn network(&self, id: &str) -> Result<&Network, RegistryError> {
        self.networks
            .get(id)
            .ok_or_else(|| missing_network(id))
    }

    pub fn asset_group(&self, id: &str) -> Result<&AssetGroup, RegistryError> {
        self.asset_groups
            .get(id)
            .ok_or_else(|| missing_asset_group(id))
    }

    pub fn asset_instrument(&self, id: &str) -> Result<&AssetInstrument, RegistryError> {
        self.asset_instruments
            .get(id)
            .ok_or_else(|| missing_asset_instrument(id))
    }

    pub fn asset_instance(&self, id: &str) -> Result<&AssetInstance, RegistryError> {
        self.asset_instances
            .get(id)
            .ok_or_else(|| missing_asset_instance(id))
    }

    pub fn asset_instances_for_group(&self, group_id: &str) -> Result<Vec<&AssetInstance>, RegistryError> {
        self.asset_group(group_id)?;
        let instrument_ids = self
            .asset_instruments
            .values()
            .filter(|instrument| instrument.group_id.as_str() == group_id)
            .map(|instrument| instrument.id.as_str())
            .collect::<Vec<_>>();
        let mut instances = self
            .asset_instances
            .values()
            .filter(|instance| instrument_ids.contains(&instance.instrument_id.as_str()))
            .collect::<Vec<_>>();
        instances.sort_by(|left, right| left.id.as_str().cmp(right.id.as_str()));
        Ok(instances)
    }

    fn validate_references(&self) -> Result<(), RegistryError> {
        for network in self.networks.values() {
            if !self.chains.contains_key(network.chain.as_str()) {
                return Err(RegistryError::MissingChain(network.chain.clone()));
            }
        }

        for instrument in self.asset_instruments.values() {
            if !self.asset_groups.contains_key(instrument.group_id.as_str()) {
                return Err(RegistryError::MissingAssetGroup(instrument.group_id.clone()));
            }
        }

        for instance in self.asset_instances.values() {
            if !self.networks.contains_key(instance.network.as_str()) {
                return Err(RegistryError::MissingNetwork(instance.network.clone()));
            }
            if !self.asset_instruments.contains_key(instance.instrument_id.as_str()) {
                return Err(RegistryError::MissingAssetInstrument(instance.instrument_id.clone()));
            }
            if let Err(err) = instance.validate_shape() {
                return Err(RegistryError::InvalidReference {
                    message: format!("asset instance {} shape invalid: {err}", instance.id),
                });
            }
        }

        for network in self.networks.values() {
            let native = self.asset_instances.get(network.native_asset_instance_id.as_str());
            match native {
                Some(instance) if instance.network == network.id => {}
                Some(_) => {
                    return Err(RegistryError::InvalidReference {
                        message: format!(
                            "network {} native asset {} belongs to another network",
                            network.id, network.native_asset_instance_id
                        ),
                    });
                }
                None => return Err(RegistryError::MissingAssetInstance(network.native_asset_instance_id.clone())),
            }
        }

        Ok(())
    }
}

fn reject_unknown_version(version: u32) -> Result<(), RegistryError> {
    if version == LATEST_REGISTRY_VERSION {
        Ok(())
    } else {
        Err(RegistryError::UnsupportedVersion { version })
    }
}

fn missing_network(id: &str) -> RegistryError {
    NetworkId::new(id)
        .map(RegistryError::MissingNetwork)
        .unwrap_or_else(|_| RegistryError::InvalidReference {
            message: "network lookup id must not be empty".to_string(),
        })
}

fn missing_asset_group(id: &str) -> RegistryError {
    AssetGroupId::new(id)
        .map(RegistryError::MissingAssetGroup)
        .unwrap_or_else(|_| RegistryError::InvalidReference {
            message: "asset group lookup id must not be empty".to_string(),
        })
}

fn missing_asset_instrument(id: &str) -> RegistryError {
    AssetInstrumentId::new(id)
        .map(RegistryError::MissingAssetInstrument)
        .unwrap_or_else(|_| RegistryError::InvalidReference {
            message: "asset instrument lookup id must not be empty".to_string(),
        })
}

fn missing_asset_instance(id: &str) -> RegistryError {
    AssetInstanceId::new(id)
        .map(RegistryError::MissingAssetInstance)
        .unwrap_or_else(|_| RegistryError::InvalidReference {
            message: "asset instance lookup id must not be empty".to_string(),
        })
}
```

- [ ] **Step 7: Run registry tests**

Run: `cargo test -p atlas-core --test registry_validation -- --nocapture`

Expected: PASS for 3 tests.

- [ ] **Step 8: Run all current tests**

Run: `cargo test -p atlas-core`

Expected: PASS for all unit and registry validation tests.

- [ ] **Step 9: Commit**

```bash
git add crates/atlas-core/src/registry.rs crates/atlas-core/tests
git commit -m "feat(core): validate chain and asset registries"
```

## Task 5: Asset Resolution Tests

**Files:**
- Create: `crates/atlas-core/tests/asset_resolution.rs`
- Modify: `crates/atlas-core/src/registry.rs`

- [ ] **Step 1: Write resolution tests**

Write `crates/atlas-core/tests/asset_resolution.rs`:

```rust
use atlas_core::registry::{AssetRegistryDocument, ChainRegistryDocument, Registry};

fn registry() -> Registry {
    let chain_doc: ChainRegistryDocument =
        serde_json::from_str(include_str!("fixtures/chain_registry.valid.json")).unwrap();
    let asset_doc: AssetRegistryDocument =
        serde_json::from_str(include_str!("fixtures/asset_registry.valid.json")).unwrap();
    Registry::from_documents(chain_doc, asset_doc).unwrap()
}

#[test]
fn usdc_group_resolves_to_ethereum_and_base_instances() {
    let registry = registry();
    let instances = registry.asset_instances_for_group("usdc").unwrap();
    let ids = instances.iter().map(|instance| instance.id.as_str()).collect::<Vec<_>>();

    assert_eq!(ids.len(), 2);
    assert!(ids.iter().any(|id| id.starts_with("eip155:1/erc20:")));
    assert!(ids.iter().any(|id| id.starts_with("eip155:8453/erc20:")));
}

#[test]
fn eth_group_resolves_to_ethereum_and_base_native_instances() {
    let registry = registry();
    let instances = registry.asset_instances_for_group("eth").unwrap();
    let ids = instances.iter().map(|instance| instance.id.as_str()).collect::<Vec<_>>();

    assert_eq!(ids, vec!["eip155:1/native:eth", "eip155:8453/native:eth"]);
}

#[test]
fn exact_asset_instance_lookup_returns_base_usdc() {
    let registry = registry();
    let instance = registry
        .asset_instance("eip155:8453/erc20:0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913")
        .unwrap();

    assert_eq!(instance.network.as_str(), "eip155:8453");
    assert_eq!(instance.instrument_id.as_str(), "usdc.circle");
}
```

- [ ] **Step 2: Run resolution tests**

Run: `cargo test -p atlas-core --test asset_resolution -- --nocapture`

Expected: PASS for 3 tests. Task 4 already sorts `asset_instances_for_group` by id before returning.

- [ ] **Step 3: Run all registry tests**

Run: `cargo test -p atlas-core --test registry_validation --test asset_resolution -- --nocapture`

Expected: PASS for 6 tests.

- [ ] **Step 4: Commit**

```bash
git add crates/atlas-core/src/registry.rs crates/atlas-core/tests/asset_resolution.rs
git commit -m "test(core): cover asset group resolution"
```

## Task 6: Transactions, Signing, And Chain Service Boundary

**Files:**
- Create: `crates/atlas-core/src/transaction.rs`
- Create: `crates/atlas-core/src/signing.rs`
- Create: `crates/atlas-core/src/service.rs`

- [ ] **Step 1: Write transaction/signing/service smoke unit test in `service.rs`**

Create `crates/atlas-core/src/service.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        amount::RawAmount,
        id::{AccountRef, AssetInstanceId, NetworkId, SignerId},
        signing::{MockSigner, SignerProvider},
        transaction::TransferIntent,
    };
    use num_bigint::BigInt;
    use std::str::FromStr;

    #[tokio::test]
    async fn mock_evm_service_prepares_signs_and_broadcasts_exact_asset_instance() {
        let service = MockEvmService::default();
        let signer = MockSigner::new(SignerId::from_str("mock-signer").unwrap());
        let intent = TransferIntent {
            asset_instance_id: AssetInstanceId::from_str("eip155:8453/erc20:0x8335").unwrap(),
            to: "0x0000000000000000000000000000000000000001".to_string(),
            amount: RawAmount::new(BigInt::from(100_000_000u64), 6),
        };

        let unsigned = service
            .prepare_transfer(AccountRef::from_str("account-1").unwrap(), NetworkId::from_str("eip155:8453").unwrap(), intent)
            .await
            .unwrap();
        let request = service.signing_request(&unsigned).unwrap();
        let response = signer.sign(request).await.unwrap();
        let signed = service.assemble_signed_transaction(unsigned, response).unwrap();
        let broadcast = service.broadcast(signed).await.unwrap();

        assert_eq!(broadcast.tx_hash, "0xmock");
    }
}
```

- [ ] **Step 2: Run service unit test to verify it fails**

Run: `cargo test -p atlas-core service::tests -- --nocapture`

Expected: FAIL because transaction, signing, and service types are not defined.

- [ ] **Step 3: Implement transaction models**

Write `crates/atlas-core/src/transaction.rs`:

```rust
use crate::{
    amount::RawAmount,
    id::{AccountRef, AssetInstanceId, NetworkId},
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TransferIntent {
    #[serde(rename = "assetInstanceId")]
    pub asset_instance_id: AssetInstanceId,
    pub to: String,
    pub amount: RawAmount,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct UnsignedTransaction {
    pub account: AccountRef,
    pub network: NetworkId,
    pub intent: TransferIntent,
    pub payload: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SignedTransaction {
    pub network: NetworkId,
    pub raw: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BroadcastResult {
    #[serde(rename = "txHash")]
    pub tx_hash: String,
}
```

- [ ] **Step 4: Implement signing models and mock signer**

Write `crates/atlas-core/src/signing.rs`:

```rust
use crate::{
    chain::Curve,
    error::SigningError,
    id::{AccountRef, NetworkId, SignerId},
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SignerRef {
    pub id: SignerId,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SigningRequest {
    pub account: AccountRef,
    pub network: NetworkId,
    pub curve: Curve,
    #[serde(rename = "payloadKind")]
    pub payload_kind: SigningPayloadKind,
    pub payload: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SigningPayloadKind {
    TransactionDigest,
    UnsignedTransaction,
    Message,
    TypedData,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum SigningResponse {
    SignatureOnly {
        signer: SignerId,
        signature: Vec<u8>,
        public_key: Vec<u8>,
    },
    SignedTransaction {
        signer: SignerId,
        raw: Vec<u8>,
    },
    SubmittedTransaction {
        signer: SignerId,
        tx_hash: String,
    },
}

#[async_trait]
pub trait SignerProvider: Send + Sync {
    fn id(&self) -> &SignerId;

    async fn sign(&self, request: SigningRequest) -> Result<SigningResponse, SigningError>;
}

#[derive(Clone, Debug)]
pub struct MockSigner {
    id: SignerId,
}

impl MockSigner {
    pub fn new(id: SignerId) -> Self {
        Self { id }
    }
}

#[async_trait]
impl SignerProvider for MockSigner {
    fn id(&self) -> &SignerId {
        &self.id
    }

    async fn sign(&self, request: SigningRequest) -> Result<SigningResponse, SigningError> {
        if request.payload.is_empty() {
            return Err(SigningError::UnsupportedPayload("empty payload".to_string()));
        }
        Ok(SigningResponse::SignatureOnly {
            signer: self.id.clone(),
            signature: b"mock-signature".to_vec(),
            public_key: b"mock-public-key".to_vec(),
        })
    }
}
```

- [ ] **Step 5: Implement service trait and mock EVM service**

Replace `crates/atlas-core/src/service.rs` with:

```rust
use crate::{
    chain::Curve,
    error::ChainError,
    id::{AccountRef, NetworkId},
    signing::{SigningPayloadKind, SigningRequest, SigningResponse},
    transaction::{BroadcastResult, SignedTransaction, TransferIntent, UnsignedTransaction},
};
use async_trait::async_trait;

#[async_trait]
pub trait ChainService: Send + Sync {
    async fn prepare_transfer(
        &self,
        account: AccountRef,
        network: NetworkId,
        intent: TransferIntent,
    ) -> Result<UnsignedTransaction, ChainError>;

    fn signing_request(&self, unsigned: &UnsignedTransaction) -> Result<SigningRequest, ChainError>;

    fn assemble_signed_transaction(
        &self,
        unsigned: UnsignedTransaction,
        response: SigningResponse,
    ) -> Result<SignedTransaction, ChainError>;

    async fn broadcast(&self, signed: SignedTransaction) -> Result<BroadcastResult, ChainError>;
}

#[derive(Clone, Debug, Default)]
pub struct MockEvmService;

#[async_trait]
impl ChainService for MockEvmService {
    async fn prepare_transfer(
        &self,
        account: AccountRef,
        network: NetworkId,
        intent: TransferIntent,
    ) -> Result<UnsignedTransaction, ChainError> {
        if !intent.asset_instance_id.as_str().starts_with(network.as_str()) {
            return Err(ChainError::UnsupportedAssetInstance(intent.asset_instance_id));
        }
        Ok(UnsignedTransaction {
            account,
            network,
            intent,
            payload: b"mock-unsigned-evm-transaction".to_vec(),
        })
    }

    fn signing_request(&self, unsigned: &UnsignedTransaction) -> Result<SigningRequest, ChainError> {
        Ok(SigningRequest {
            account: unsigned.account.clone(),
            network: unsigned.network.clone(),
            curve: Curve::Secp256k1,
            payload_kind: SigningPayloadKind::TransactionDigest,
            payload: b"mock-digest".to_vec(),
        })
    }

    fn assemble_signed_transaction(
        &self,
        unsigned: UnsignedTransaction,
        response: SigningResponse,
    ) -> Result<SignedTransaction, ChainError> {
        match response {
            SigningResponse::SignatureOnly { signature, .. } => {
                let mut raw = unsigned.payload;
                raw.extend(signature);
                Ok(SignedTransaction {
                    network: unsigned.network,
                    raw,
                })
            }
            SigningResponse::SignedTransaction { raw, .. } => Ok(SignedTransaction {
                network: unsigned.network,
                raw,
            }),
            SigningResponse::SubmittedTransaction { tx_hash, .. } => Err(ChainError::TransactionBuildFailed(format!(
                "mock service expected signed bytes, got submitted hash {tx_hash}"
            ))),
        }
    }

    async fn broadcast(&self, signed: SignedTransaction) -> Result<BroadcastResult, ChainError> {
        if signed.raw.is_empty() {
            return Err(ChainError::BroadcastFailed("empty signed transaction".to_string()));
        }
        Ok(BroadcastResult {
            tx_hash: "0xmock".to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        amount::RawAmount,
        id::{AccountRef, AssetInstanceId, NetworkId, SignerId},
        signing::{MockSigner, SignerProvider},
        transaction::TransferIntent,
    };
    use num_bigint::BigInt;
    use std::str::FromStr;

    #[tokio::test]
    async fn mock_evm_service_prepares_signs_and_broadcasts_exact_asset_instance() {
        let service = MockEvmService;
        let signer = MockSigner::new(SignerId::from_str("mock-signer").unwrap());
        let intent = TransferIntent {
            asset_instance_id: AssetInstanceId::from_str("eip155:8453/erc20:0x8335").unwrap(),
            to: "0x0000000000000000000000000000000000000001".to_string(),
            amount: RawAmount::new(BigInt::from(100_000_000u64), 6),
        };

        let unsigned = service
            .prepare_transfer(AccountRef::from_str("account-1").unwrap(), NetworkId::from_str("eip155:8453").unwrap(), intent)
            .await
            .unwrap();
        let request = service.signing_request(&unsigned).unwrap();
        let response = signer.sign(request).await.unwrap();
        let signed = service.assemble_signed_transaction(unsigned, response).unwrap();
        let broadcast = service.broadcast(signed).await.unwrap();

        assert_eq!(broadcast.tx_hash, "0xmock");
    }
}
```

- [ ] **Step 6: Run service tests**

Run: `cargo test -p atlas-core service::tests -- --nocapture`

Expected: PASS for the mock EVM flow test.

- [ ] **Step 7: Run all tests**

Run: `cargo test -p atlas-core`

Expected: PASS for all current tests.

- [ ] **Step 8: Commit**

```bash
git add crates/atlas-core/src/transaction.rs crates/atlas-core/src/signing.rs crates/atlas-core/src/service.rs
git commit -m "feat(core): define signing and chain service boundary"
```

## Task 7: End-To-End Smoke Test

**Files:**
- Create: `crates/atlas-core/tests/smoke_flow.rs`

- [ ] **Step 1: Write smoke test**

Write `crates/atlas-core/tests/smoke_flow.rs`:

```rust
use atlas_core::{
    amount::RawAmount,
    id::{AccountRef, AssetInstanceId, NetworkId, SignerId},
    registry::{AssetRegistryDocument, ChainRegistryDocument, Registry},
    service::{ChainService, MockEvmService},
    signing::{MockSigner, SignerProvider},
    transaction::TransferIntent,
};
use num_bigint::BigInt;
use std::str::FromStr;

fn registry() -> Registry {
    let chain_doc: ChainRegistryDocument =
        serde_json::from_str(include_str!("fixtures/chain_registry.valid.json")).unwrap();
    let asset_doc: AssetRegistryDocument =
        serde_json::from_str(include_str!("fixtures/asset_registry.valid.json")).unwrap();
    Registry::from_documents(chain_doc, asset_doc).unwrap()
}

#[tokio::test]
async fn base_usdc_transfer_smoke_flow_uses_exact_asset_instance() {
    let registry = registry();
    let asset = registry
        .asset_instance("eip155:8453/erc20:0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913")
        .unwrap();
    let network = registry.network(asset.network.as_str()).unwrap();

    let service = MockEvmService;
    let signer = MockSigner::new(SignerId::from_str("mock-signer").unwrap());
    let intent = TransferIntent {
        asset_instance_id: AssetInstanceId::from_str(asset.id.as_str()).unwrap(),
        to: "0x0000000000000000000000000000000000000001".to_string(),
        amount: RawAmount::new(BigInt::from(100_000_000u64), asset.decimals),
    };

    let unsigned = service
        .prepare_transfer(
            AccountRef::from_str("account-1").unwrap(),
            NetworkId::from_str(network.id.as_str()).unwrap(),
            intent,
        )
        .await
        .unwrap();
    let request = service.signing_request(&unsigned).unwrap();
    let response = signer.sign(request).await.unwrap();
    let signed = service.assemble_signed_transaction(unsigned, response).unwrap();
    let broadcast = service.broadcast(signed).await.unwrap();

    assert_eq!(broadcast.tx_hash, "0xmock");
}
```

- [ ] **Step 2: Run smoke test**

Run: `cargo test -p atlas-core --test smoke_flow -- --nocapture`

Expected: PASS for the Base USDC smoke flow.

- [ ] **Step 3: Run all tests and crate check**

Run: `cargo test -p atlas-core && cargo check -p atlas-core`

Expected: PASS for all tests and check.

- [ ] **Step 4: Commit**

```bash
git add crates/atlas-core/tests/smoke_flow.rs
git commit -m "test(core): add exact asset transfer smoke flow"
```

## Task 8: Documentation Pass

**Files:**
- Modify: `README.md`
- Create: `crates/atlas-core/README.md`

- [ ] **Step 1: Update root README**

Replace `README.md` with:

```markdown
# Atlas

Atlas is a modular Rust blockchain SDK foundation. The first scope is a strict
blockchain core: chain and asset registries, exact asset instances, precise
amount types, provider-neutral signing boundaries, and mocked EVM execution
flow.

See:

- [Repo engineering rules](CODEX.md)
- [Core blockchain layer design](docs/superpowers/specs/2026-05-08-atlas-core-blockchain-layer-design.md)
- [Core blockchain implementation plan](docs/superpowers/plans/2026-05-08-atlas-core-blockchain-layer.md)

## First Scope

- EVM chain family
- Ethereum and Base registry fixtures
- Native coins and ERC-20 fungible tokens
- Big integer raw amounts
- Typed errors
- Registry validation
- Provider-neutral signing traits
- Mocked chain service smoke flow
```

- [ ] **Step 2: Create crate README**

Write `crates/atlas-core/README.md`:

```markdown
# atlas-core

`atlas-core` contains the first Atlas blockchain primitives:

- typed ids
- raw amount model
- chain and network models
- asset group, instrument, and instance models
- split chain and asset registry documents
- registry validation and lookup
- signing request and response boundary
- chain service trait
- mock EVM service for boundary tests

Execution must always use `AssetInstance`. `AssetGroup` and `AssetInstrument`
are for display, search, aggregation, and routing layers.
```

- [ ] **Step 3: Run documentation-adjacent checks**

Run: `cargo test -p atlas-core && cargo check -p atlas-core`

Expected: PASS for all tests and check.

- [ ] **Step 4: Commit**

```bash
git add README.md crates/atlas-core/README.md
git commit -m "docs: describe atlas core first scope"
```

## Task 9: Final Verification

**Files:**
- No file changes expected.

- [ ] **Step 1: Run formatting**

Run: `cargo fmt --all --check`

Expected: PASS with no formatting diffs.

- [ ] **Step 2: Run all tests**

Run: `cargo test --workspace`

Expected: PASS for all workspace tests.

- [ ] **Step 3: Run workspace check**

Run: `cargo check --workspace`

Expected: PASS for all workspace crates.

- [ ] **Step 4: Inspect git status**

Run: `git status --short`

Expected: no uncommitted implementation files. Ignored `.superpowers/` may appear only when using `git status --ignored`.

## Self-Review Notes

- Spec coverage: This plan covers strict blockchain core, chain/network/asset models, split registries, exact `AssetInstance` execution, provider-neutral signing traits, precision rules, typed errors, fixture tests, smoke tests, and documentation. Product accounts are preserved in `docs/superpowers/specs/2026-05-08-atlas-product-accounts-composition-notes.md`; portfolio APIs, concrete signer providers, non-EVM chains, public RPC integration, and Wallet Core integration remain future implementation plans as intended.
- Placeholder scan: No task uses `TBD`, `TODO`, or open-ended implementation instructions. Each code-writing step includes exact file content or exact replacement snippets.
- Type consistency: `AssetInstanceId`, `NetworkId`, `Registry`, `TransferIntent`, `SigningRequest`, `SigningResponse`, `ChainService`, and `MockEvmService` names are consistent across tasks.
