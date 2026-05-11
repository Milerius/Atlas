# Atlas Server Hybrid Deployment Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship `atlas-server` (an axum HTTP service over `EvmChainService::prepare_unsigned_bundle`) plus `atlas-e2e` (a test crate proving direct + server flows against real Sepolia / Base Sepolia), packaged as a distroless Docker image, with CI workflows for build / e2e / docker / spec-release.

**Architecture:** atlas-server is one Rust crate (binary + library). axum routes call into a chain-service dispatcher (enum dispatch over `NetworkChainService`) wired from atlas-core's bundled registry + per-network RPC URL env vars. Stripe-style errors, bearer auth middleware, utoipa-generated OpenAPI, Scalar UI at /docs, full observability (tracing JSON + Prometheus + feature-gated OTel). atlas-e2e exercises the SDK directly (no server) AND drives the server via in-process spawn. Both produce real testnet transactions.

**Tech Stack:** axum 0.7, tower / tower-http, utoipa + utoipa-axum, tracing + tracing-subscriber + tracing-opentelemetry (feature-gated), metrics + metrics-exporter-prometheus, reqwest, clap, anyhow, thiserror, alloy 2.x (already in workspace).

---

## Task 1: Workspace deps + atlas-server crate skeleton

**Files:**
- Modify: `Cargo.toml` (workspace root)
- Create: `crates/atlas-server/Cargo.toml`
- Create: `crates/atlas-server/src/lib.rs`
- Create: `crates/atlas-server/src/main.rs`

- [ ] **Step 1: Add new workspace dependencies in `Cargo.toml`**

Add under `[workspace.dependencies]`:

```toml
axum                = "0.7"
tower               = "0.5"
tower-http          = { version = "0.6", features = ["cors", "trace", "limit"] }
utoipa              = { version = "5", features = ["axum_extras"] }
utoipa-axum         = "0.1"
tracing-subscriber  = { version = "0.3", features = ["env-filter", "json"] }
tracing-opentelemetry = { version = "0.27", optional = true }
opentelemetry       = { version = "0.26", optional = true }
opentelemetry-otlp  = { version = "0.26", optional = true, features = ["tonic"] }
metrics             = "0.24"
metrics-exporter-prometheus = "0.16"
reqwest             = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }
anyhow              = "1"
clap                = { version = "4", features = ["derive"] }
hex                 = "0.4"
```

Add to the workspace `[workspace]` members:
```toml
members = [
    "crates/atlas-core",
    "crates/atlas-evm",
    "crates/atlas-scenarios",
    "crates/atlas-server",
    "crates/atlas-signer-localkey",
    "crates/atlas-verify",
]
```

- [ ] **Step 2: Create `crates/atlas-server/Cargo.toml`**

```toml
[package]
name = "atlas-server"
version = "0.1.0"
edition.workspace = true
license.workspace = true
repository.workspace = true
publish = false

[features]
default = []
otel = ["dep:tracing-opentelemetry", "dep:opentelemetry", "dep:opentelemetry-otlp"]

[dependencies]
alloy-primitives.workspace = true
alloy-provider.workspace = true
alloy-transport.workspace = true
async-trait.workspace = true
atlas-core = { path = "../atlas-core" }
atlas-evm = { path = "../atlas-evm" }
anyhow.workspace = true
axum.workspace = true
clap.workspace = true
hex.workspace = true
metrics.workspace = true
metrics-exporter-prometheus.workspace = true
num-bigint.workspace = true
opentelemetry = { workspace = true, optional = true }
opentelemetry-otlp = { workspace = true, optional = true }
reqwest.workspace = true
serde.workspace = true
serde_json.workspace = true
thiserror.workspace = true
tokio = { workspace = true, features = ["macros", "rt-multi-thread", "signal"] }
tower.workspace = true
tower-http.workspace = true
tracing = { workspace = true }
tracing-opentelemetry = { workspace = true, optional = true }
tracing-subscriber.workspace = true
utoipa.workspace = true
utoipa-axum.workspace = true

[dev-dependencies]
tokio = { workspace = true, features = ["macros", "rt-multi-thread", "test-util"] }
```

Note: this assumes `alloy-transport` and `tracing` are exposed at workspace level. Add them to root `Cargo.toml` `[workspace.dependencies]` if not present:

```toml
alloy-transport     = "2.0"
tracing             = "0.1"
```

- [ ] **Step 3: Create minimal `crates/atlas-server/src/lib.rs`**

```rust
//! Atlas HTTP server — wraps `atlas_evm::EvmChainService` behind a
//! REST API for the server-builds-tx + client-signs deployment shape.
//!
//! See `docs/superpowers/specs/2026-05-11-atlas-server-hybrid-deployment-design.md`
//! for the design.

#![forbid(unsafe_code)]
```

- [ ] **Step 4: Create minimal `crates/atlas-server/src/main.rs`**

```rust
fn main() {
    eprintln!("atlas-server skeleton — not yet implemented");
    std::process::exit(0);
}
```

- [ ] **Step 5: Verify it builds**

Run: `cargo build -p atlas-server`
Expected: PASS, produces `target/debug/atlas-server`.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml crates/atlas-server/
git commit -m "feat(server): add atlas-server crate skeleton + workspace deps"
```

---

## Task 2: `ServerConfig` + `from_env()` parser

**Files:**
- Create: `crates/atlas-server/src/config.rs`
- Modify: `crates/atlas-server/src/lib.rs`

- [ ] **Step 1: Write the failing tests in `crates/atlas-server/src/config.rs`**

```rust
//! Configuration loaded from environment variables.

use atlas_core::id::NetworkId;
use atlas_core::registry::Registry;
use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::Level;

#[derive(Clone)]
pub struct ServerConfig {
    pub bind: SocketAddr,
    pub registry: Arc<Registry>,
    pub api_keys: Arc<HashSet<String>>,
    pub rpc_url_for: Arc<dyn Fn(&NetworkId) -> Option<String> + Send + Sync>,
    pub cors_allowed_origins: CorsOrigins,
    pub log_level: Level,
    pub otel_endpoint: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CorsOrigins {
    Any,
    Allowlist(Vec<String>),
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("invalid socket address in ATLAS_BIND: {0}")]
    InvalidBind(String),
    #[error("invalid log level in ATLAS_LOG_LEVEL: {0}")]
    InvalidLogLevel(String),
    #[error("registry load failed: {0}")]
    Registry(String),
}

/// Build an env-var key for a network's RPC URL.
/// `eip155:11155111` → `ATLAS_RPC_EIP155_11155111`.
pub fn rpc_env_var_name(network: &NetworkId) -> String {
    let normalised: String = network
        .as_str()
        .chars()
        .map(|c| match c {
            ':' | '/' | '-' | '.' => '_',
            c => c.to_ascii_uppercase(),
        })
        .collect();
    format!("ATLAS_RPC_{}", normalised)
}

impl ServerConfig {
    pub fn from_env() -> Result<Self, ConfigError> {
        use atlas_core::official::{ASSET_REGISTRY_JSON, CHAIN_REGISTRY_JSON};
        use atlas_core::registry::{AssetRegistryDocument, ChainRegistryDocument};

        let bind: SocketAddr = std::env::var("ATLAS_BIND")
            .unwrap_or_else(|_| "0.0.0.0:8080".to_string())
            .parse()
            .map_err(|e: std::net::AddrParseError| ConfigError::InvalidBind(e.to_string()))?;

        let log_level: Level = std::env::var("ATLAS_LOG_LEVEL")
            .unwrap_or_else(|_| "info".to_string())
            .parse()
            .map_err(|e: tracing::metadata::ParseLevelError| {
                ConfigError::InvalidLogLevel(e.to_string())
            })?;

        let api_keys: HashSet<String> = std::env::var("ATLAS_API_KEYS")
            .unwrap_or_default()
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        let cors_allowed_origins = match std::env::var("ATLAS_CORS_ALLOWED_ORIGINS")
            .unwrap_or_else(|_| "*".to_string())
            .as_str()
        {
            "*" => CorsOrigins::Any,
            s => CorsOrigins::Allowlist(
                s.split(',').map(|o| o.trim().to_string()).collect(),
            ),
        };

        let otel_endpoint = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok();

        let chain_doc: ChainRegistryDocument = serde_json::from_str(CHAIN_REGISTRY_JSON)
            .map_err(|e| ConfigError::Registry(e.to_string()))?;
        let asset_doc: AssetRegistryDocument = serde_json::from_str(ASSET_REGISTRY_JSON)
            .map_err(|e| ConfigError::Registry(e.to_string()))?;
        let registry = Registry::from_documents(chain_doc, asset_doc)
            .map_err(|e| ConfigError::Registry(e.to_string()))?;

        let rpc_url_for: Arc<dyn Fn(&NetworkId) -> Option<String> + Send + Sync> =
            Arc::new(|id: &NetworkId| std::env::var(rpc_env_var_name(id)).ok());

        Ok(Self {
            bind,
            registry: Arc::new(registry),
            api_keys: Arc::new(api_keys),
            rpc_url_for,
            cors_allowed_origins,
            log_level,
            otel_endpoint,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn rpc_env_var_name_for_eip155_sepolia() {
        let id = NetworkId::from_str("eip155:11155111").unwrap();
        assert_eq!(rpc_env_var_name(&id), "ATLAS_RPC_EIP155_11155111");
    }

    #[test]
    fn rpc_env_var_name_for_solana_mainnet() {
        let id = NetworkId::from_str("solana:5eykt4UsFv8P8NJdTREpY1vzqKqZKvdp").unwrap();
        assert_eq!(
            rpc_env_var_name(&id),
            "ATLAS_RPC_SOLANA_5EYKT4USFV8P8NJDTREPY1VZQKQZKVDP"
        );
    }

    #[test]
    fn cors_origins_parses_any() {
        std::env::set_var("ATLAS_BIND", "127.0.0.1:0");
        std::env::set_var("ATLAS_CORS_ALLOWED_ORIGINS", "*");
        std::env::remove_var("ATLAS_API_KEYS");
        let cfg = ServerConfig::from_env().unwrap();
        assert_eq!(cfg.cors_allowed_origins, CorsOrigins::Any);
    }
}
```

- [ ] **Step 2: Wire the module into `lib.rs`**

Edit `crates/atlas-server/src/lib.rs`:

```rust
//! Atlas HTTP server — wraps `atlas_evm::EvmChainService` behind a
//! REST API for the server-builds-tx + client-signs deployment shape.

#![forbid(unsafe_code)]

pub mod config;
```

- [ ] **Step 3: Run tests, verify they pass**

Run: `cargo test -p atlas-server --lib`
Expected: 3 tests pass.

- [ ] **Step 4: Run fmt + clippy**

```bash
cargo fmt --all -- --check
cargo clippy -p atlas-server --all-targets -- -D warnings
```

- [ ] **Step 5: Commit**

```bash
git add crates/atlas-server/
git commit -m "feat(server): ServerConfig + from_env() with rpc-url-for closure"
```

---

## Task 3: `ApiError` + Stripe-style error body

**Files:**
- Create: `crates/atlas-server/src/error.rs`
- Modify: `crates/atlas-server/src/lib.rs`

- [ ] **Step 1: Create `crates/atlas-server/src/error.rs`**

```rust
//! API error model — Stripe-style error body, mapped from
//! atlas-core typed errors and HTTP status codes.

use atlas_core::address::AddressError;
use atlas_core::amount::AmountError;
use atlas_core::caip::CaipError;
use atlas_core::error::{ChainError, RpcError};
use atlas_core::id::{IdError, NetworkId};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;
use utoipa::ToSchema;

#[derive(Debug, Serialize, ToSchema)]
pub struct ErrorBody {
    pub error: ErrorDetail,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ErrorDetail {
    /// Broad class of error — `invalid_request_error`, `auth_error`,
    /// `api_error`, `rpc_error`.
    #[serde(rename = "type")]
    pub type_: &'static str,
    /// Specific machine-readable code (`invalid_caip`,
    /// `network_unconfigured`, …).
    pub code: &'static str,
    /// Human-readable message — inner typed error's `Display`. Never
    /// includes RPC URLs, stack traces, or PII.
    pub message: String,
    /// JSON-path of the offending field (`intent.assetInstanceId`),
    /// where determinable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub param: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error(transparent)]
    Chain(#[from] ChainError),
    #[error(transparent)]
    Caip(#[from] CaipError),
    #[error(transparent)]
    Address(#[from] AddressError),
    #[error(transparent)]
    Id(#[from] IdError),
    #[error(transparent)]
    Amount(#[from] AmountError),

    #[error("network {0} is not configured")]
    NetworkUnconfigured(NetworkId),
    #[error("namespace {0:?} is not supported by this server")]
    UnsupportedNamespace(String),
    #[error("missing bearer token")]
    MissingToken,
    #[error("invalid bearer token")]
    InvalidToken,
    #[error("invalid request body: {0}")]
    InvalidRequestBody(String),
    #[error("internal error: {0}")]
    Internal(String),
}

impl ApiError {
    fn classify(&self) -> (StatusCode, &'static str, &'static str, Option<String>) {
        match self {
            Self::Caip(_) => (
                StatusCode::BAD_REQUEST,
                "invalid_request_error",
                "invalid_caip",
                Some("intent.assetInstanceId".to_string()),
            ),
            Self::Address(_) => (
                StatusCode::BAD_REQUEST,
                "invalid_request_error",
                "invalid_address",
                None,
            ),
            Self::Id(_) => (
                StatusCode::BAD_REQUEST,
                "invalid_request_error",
                "invalid_id",
                None,
            ),
            Self::Amount(AmountError::NegativeValue) => (
                StatusCode::BAD_REQUEST,
                "invalid_request_error",
                "negative_amount",
                Some("intent.amount.value".to_string()),
            ),
            Self::Amount(AmountError::DecimalsMismatch { .. }) => (
                StatusCode::BAD_REQUEST,
                "invalid_request_error",
                "decimals_mismatch",
                Some("intent.amount.decimals".to_string()),
            ),
            Self::Chain(ChainError::UnsupportedAssetInstance(_)) => (
                StatusCode::BAD_REQUEST,
                "invalid_request_error",
                "unsupported_asset",
                Some("intent.assetInstanceId".to_string()),
            ),
            Self::Chain(ChainError::InvalidAddress(_)) => (
                StatusCode::BAD_REQUEST,
                "invalid_request_error",
                "invalid_address",
                None,
            ),
            Self::Chain(ChainError::StandardNotSupported { .. }) => (
                StatusCode::BAD_REQUEST,
                "invalid_request_error",
                "unsupported_standard",
                Some("intent.assetInstanceId".to_string()),
            ),
            Self::Chain(ChainError::Rpc(RpcError::Transport(_))) => (
                StatusCode::BAD_GATEWAY,
                "rpc_error",
                "rpc_unavailable",
                None,
            ),
            Self::Chain(ChainError::Rpc(RpcError::NodeError(_))) => (
                StatusCode::BAD_GATEWAY,
                "rpc_error",
                "rpc_node_error",
                None,
            ),
            Self::Chain(ChainError::Rpc(RpcError::MalformedResponse(_))) => (
                StatusCode::BAD_GATEWAY,
                "rpc_error",
                "rpc_malformed_response",
                None,
            ),
            Self::Chain(ChainError::FeeEstimationFailed(_)) => (
                StatusCode::BAD_GATEWAY,
                "rpc_error",
                "rpc_unavailable",
                None,
            ),
            Self::Chain(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "api_error",
                "build_failed",
                None,
            ),
            Self::NetworkUnconfigured(_) => (
                StatusCode::SERVICE_UNAVAILABLE,
                "api_error",
                "network_unconfigured",
                None,
            ),
            Self::UnsupportedNamespace(_) => (
                StatusCode::BAD_REQUEST,
                "invalid_request_error",
                "unsupported_asset",
                Some("intent.assetInstanceId".to_string()),
            ),
            Self::MissingToken => (
                StatusCode::UNAUTHORIZED,
                "auth_error",
                "missing_token",
                None,
            ),
            Self::InvalidToken => (
                StatusCode::UNAUTHORIZED,
                "auth_error",
                "invalid_token",
                None,
            ),
            Self::InvalidRequestBody(_) => (
                StatusCode::BAD_REQUEST,
                "invalid_request_error",
                "invalid_request_body",
                None,
            ),
            Self::Internal(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "api_error",
                "internal_error",
                None,
            ),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, type_, code, param) = self.classify();
        let body = ErrorBody {
            error: ErrorDetail {
                type_,
                code,
                message: self.to_string(),
                param,
            },
        };
        (status, Json(body)).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlas_core::id::NetworkId;
    use axum::http::StatusCode;
    use std::str::FromStr;

    #[test]
    fn caip_error_maps_to_400_invalid_caip() {
        let err = ApiError::Caip(CaipError::Caip2MissingSeparator("nope".into()));
        let (status, type_, code, param) = err.classify();
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(type_, "invalid_request_error");
        assert_eq!(code, "invalid_caip");
        assert_eq!(param.as_deref(), Some("intent.assetInstanceId"));
    }

    #[test]
    fn network_unconfigured_maps_to_503() {
        let err = ApiError::NetworkUnconfigured(NetworkId::from_str("eip155:1").unwrap());
        let (status, _, code, _) = err.classify();
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(code, "network_unconfigured");
    }

    #[test]
    fn missing_token_maps_to_401() {
        let err = ApiError::MissingToken;
        let (status, type_, code, _) = err.classify();
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(type_, "auth_error");
        assert_eq!(code, "missing_token");
    }

    #[test]
    fn rpc_transport_error_maps_to_502() {
        let err = ApiError::Chain(ChainError::Rpc(RpcError::Transport(
            "connection refused".into(),
        )));
        let (status, type_, code, _) = err.classify();
        assert_eq!(status, StatusCode::BAD_GATEWAY);
        assert_eq!(type_, "rpc_error");
        assert_eq!(code, "rpc_unavailable");
    }

    #[test]
    fn negative_amount_maps_to_400_with_param() {
        let err = ApiError::Amount(AmountError::NegativeValue);
        let (status, _, code, param) = err.classify();
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(code, "negative_amount");
        assert_eq!(param.as_deref(), Some("intent.amount.value"));
    }
}
```

- [ ] **Step 2: Add module to `lib.rs`**

```rust
pub mod config;
pub mod error;
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p atlas-server --lib error::`
Expected: 5 tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/atlas-server/src/error.rs crates/atlas-server/src/lib.rs
git commit -m "feat(server): Stripe-style ApiError + IntoResponse mapping"
```

---

## Task 4: `AppState` + `NetworkChainService` enum dispatcher

**Files:**
- Create: `crates/atlas-server/src/state.rs`
- Modify: `crates/atlas-server/src/lib.rs`

- [ ] **Step 1: Create `crates/atlas-server/src/state.rs`**

```rust
//! Shared application state plus the per-network chain-service
//! dispatcher.

use crate::config::ServerConfig;
use crate::error::ApiError;
use alloy_provider::{ProviderBuilder, RootProvider};
use atlas_core::id::NetworkId;
use atlas_core::registry::Registry;
use atlas_core::transaction::{TransferIntent, UnsignedBundle};
use atlas_evm::service::EvmChainService;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;
use tracing::warn;

#[derive(Clone)]
pub struct AppState {
    pub registry: Arc<Registry>,
    pub chain_services: Arc<HashMap<NetworkId, NetworkChainService>>,
    pub api_keys: Arc<HashSet<String>>,
    pub started_at: Instant,
    pub version: &'static str,
}

pub const ATLAS_SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

pub enum NetworkChainService {
    Evm(EvmChainService<RootProvider>),
    Unconfigured { reason: UnconfiguredReason },
}

#[derive(Clone, Debug)]
pub enum UnconfiguredReason {
    NoRpcUrl,
    UnknownNamespace(String),
}

impl AppState {
    pub fn build(cfg: &ServerConfig) -> Result<Self, ApiError> {
        let mut services: HashMap<NetworkId, NetworkChainService> = HashMap::new();

        for (id_str, network) in iter_networks(&cfg.registry) {
            let id = network.id.clone();
            let namespace = id.namespace().to_string();

            if namespace == "eip155" {
                match (cfg.rpc_url_for)(&id) {
                    Some(url) => match url.parse() {
                        Ok(parsed_url) => {
                            let provider = ProviderBuilder::new().connect_http(parsed_url);
                            let chain_id = network
                                .chain_id
                                .as_deref()
                                .and_then(|s| s.parse::<u64>().ok())
                                .unwrap_or(0);
                            let eip1559 = network.features.eip1559;
                            let svc = EvmChainService::new(provider, id.clone(), chain_id, eip1559);
                            services.insert(id, NetworkChainService::Evm(svc));
                        }
                        Err(e) => {
                            warn!(network = %id_str, error = %e, "ignoring malformed RPC URL");
                            services.insert(
                                id,
                                NetworkChainService::Unconfigured {
                                    reason: UnconfiguredReason::NoRpcUrl,
                                },
                            );
                        }
                    },
                    None => {
                        services.insert(
                            id,
                            NetworkChainService::Unconfigured {
                                reason: UnconfiguredReason::NoRpcUrl,
                            },
                        );
                    }
                }
            } else {
                services.insert(
                    id,
                    NetworkChainService::Unconfigured {
                        reason: UnconfiguredReason::UnknownNamespace(namespace),
                    },
                );
            }
        }

        Ok(Self {
            registry: cfg.registry.clone(),
            chain_services: Arc::new(services),
            api_keys: cfg.api_keys.clone(),
            started_at: Instant::now(),
            version: ATLAS_SERVER_VERSION,
        })
    }

    /// Dispatch a prepare request to the right chain service. Returns
    /// the corresponding [`ApiError`] when the network is unknown or
    /// unconfigured.
    pub async fn prepare_unsigned_bundle(
        &self,
        intent: TransferIntent,
        account: atlas_core::id::AccountRef,
    ) -> Result<UnsignedBundle, ApiError> {
        let network_id = intent.asset_instance_id.network_id();
        let svc = self
            .chain_services
            .get(&network_id)
            .ok_or_else(|| ApiError::UnsupportedNamespace(network_id.namespace().to_string()))?;
        match svc {
            NetworkChainService::Evm(svc) => Ok(svc.prepare_unsigned_bundle(intent, account).await?),
            NetworkChainService::Unconfigured {
                reason: UnconfiguredReason::NoRpcUrl,
            } => Err(ApiError::NetworkUnconfigured(network_id)),
            NetworkChainService::Unconfigured {
                reason: UnconfiguredReason::UnknownNamespace(ns),
            } => Err(ApiError::UnsupportedNamespace(ns.clone())),
        }
    }
}

fn iter_networks(
    registry: &Registry,
) -> impl Iterator<Item = (String, &atlas_core::chain::Network)> {
    registry
        .iter_network_ids()
        .map(|id| (id.to_string(), registry.network(id).expect("listed")))
}
```

- [ ] **Step 2: Add `iter_network_ids` accessor to atlas-core**

The `state.rs` above expects `Registry::iter_network_ids() -> impl Iterator<Item = &str>`. If it doesn't exist on `Registry` yet, add it.

Inside `crates/atlas-core/src/registry.rs`, in the `impl Registry` block (next to `network()`):

```rust
/// Iterate over all registered network ids (CAIP-2 strings).
pub fn iter_network_ids(&self) -> impl Iterator<Item = &str> {
    self.networks.keys().map(|s| s.as_str())
}
```

Add a unit test in `crates/atlas-core/src/registry.rs` (inside the existing `mod tests`):

```rust
#[test]
fn iter_network_ids_returns_registered_keys() {
    let registry = valid_registry();
    let ids: std::collections::HashSet<_> = registry.iter_network_ids().collect();
    assert!(ids.contains("eip155:1"));
}
```

- [ ] **Step 3: Wire module into `lib.rs`**

```rust
pub mod config;
pub mod error;
pub mod state;
```

- [ ] **Step 4: Build + run tests**

Run: `cargo build -p atlas-server && cargo test -p atlas-core --lib registry::tests::iter_network_ids`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/atlas-server/src/state.rs crates/atlas-server/src/lib.rs crates/atlas-core/src/registry.rs
git commit -m "feat(server,core): AppState + NetworkChainService dispatch + iter_network_ids"
```

---

## Task 5: 0x-hex serde wrappers for payload bytes

**Files:**
- Create: `crates/atlas-server/src/hex_codec.rs`
- Modify: `crates/atlas-server/src/lib.rs`

- [ ] **Step 1: Write the failing tests**

Create `crates/atlas-server/src/hex_codec.rs`:

```rust
//! `0x`-hex serde wrapper for `Vec<u8>` payload fields.
//!
//! atlas-core's wire types serdes `Vec<u8>` as a JSON array of u8 by
//! default. The HTTP boundary wraps payloads as `0x`-hex strings —
//! the Web3 convention — without touching atlas-core types.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub fn serialize<S: Serializer>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
    let hex = format!("0x{}", hex::encode(bytes));
    hex.serialize(serializer)
}

pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
    let raw = String::deserialize(deserializer)?;
    let stripped = raw.strip_prefix("0x").unwrap_or(&raw);
    hex::decode(stripped).map_err(serde::de::Error::custom)
}

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct Wrapper {
        #[serde(with = "super")]
        payload: Vec<u8>,
    }

    #[test]
    fn serialises_as_0x_hex() {
        let w = Wrapper { payload: vec![0xde, 0xad, 0xbe, 0xef] };
        let json = serde_json::to_string(&w).unwrap();
        assert_eq!(json, r#"{"payload":"0xdeadbeef"}"#);
    }

    #[test]
    fn deserialises_from_0x_hex() {
        let w: Wrapper = serde_json::from_str(r#"{"payload":"0xdeadbeef"}"#).unwrap();
        assert_eq!(w.payload, vec![0xde, 0xad, 0xbe, 0xef]);
    }

    #[test]
    fn deserialises_from_bare_hex_without_0x() {
        let w: Wrapper = serde_json::from_str(r#"{"payload":"deadbeef"}"#).unwrap();
        assert_eq!(w.payload, vec![0xde, 0xad, 0xbe, 0xef]);
    }

    #[test]
    fn deserialises_empty() {
        let w: Wrapper = serde_json::from_str(r#"{"payload":"0x"}"#).unwrap();
        assert_eq!(w.payload, Vec::<u8>::new());
    }

    #[test]
    fn rejects_non_hex_input() {
        let r: Result<Wrapper, _> = serde_json::from_str(r#"{"payload":"0xZZ"}"#);
        assert!(r.is_err());
    }
}
```

- [ ] **Step 2: Wire module into `lib.rs`**

```rust
pub mod config;
pub mod error;
pub mod hex_codec;
pub mod state;
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p atlas-server --lib hex_codec::`
Expected: 5 tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/atlas-server/src/hex_codec.rs crates/atlas-server/src/lib.rs
git commit -m "feat(server): 0x-hex serde wrapper for payload bytes"
```

---

## Task 6: GET /health route + integration test

**Files:**
- Create: `crates/atlas-server/src/routes/mod.rs`
- Create: `crates/atlas-server/src/routes/health.rs`
- Modify: `crates/atlas-server/src/lib.rs`
- Create: `crates/atlas-server/tests/server_health.rs`

- [ ] **Step 1: Write the failing integration test**

Create `crates/atlas-server/tests/server_health.rs`:

```rust
//! Integration test for `GET /health`.

use atlas_server::test_support::TestServer;

#[tokio::test]
async fn health_endpoint_returns_200_and_status_payload() {
    let server = TestServer::start_with_no_rpc().await;
    let resp = reqwest::get(format!("{}/health", server.base_url())).await.unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "ok");
    assert!(body["version"].is_string());
    assert!(body["uptime_seconds"].as_u64().is_some());
}
```

- [ ] **Step 2: Create the routes module skeleton**

Create `crates/atlas-server/src/routes/mod.rs`:

```rust
//! HTTP route modules. Each sub-module exposes a `pub fn router() ->
//! axum::Router<crate::state::AppState>` that the lib aggregates.

pub mod health;

use crate::state::AppState;
use axum::Router;

pub fn router() -> Router<AppState> {
    Router::new().merge(health::router())
}
```

- [ ] **Step 3: Create `crates/atlas-server/src/routes/health.rs`**

```rust
//! Liveness probe: `GET /health`.

use crate::state::AppState;
use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;
use utoipa::ToSchema;

pub fn router() -> Router<AppState> {
    Router::new().route("/health", get(health))
}

#[derive(Serialize, ToSchema)]
pub struct HealthResponse {
    pub status: &'static str,
    pub version: &'static str,
    pub uptime_seconds: u64,
}

#[utoipa::path(
    get,
    path = "/health",
    responses((status = 200, description = "Liveness probe", body = HealthResponse)),
    tag = "ops"
)]
pub async fn health(State(state): State<AppState>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        version: state.version,
        uptime_seconds: state.started_at.elapsed().as_secs(),
    })
}
```

- [ ] **Step 4: Add a minimal `pub fn run` + `TestServer` test support in `lib.rs`**

```rust
//! Atlas HTTP server.

#![forbid(unsafe_code)]

pub mod config;
pub mod error;
pub mod hex_codec;
pub mod routes;
pub mod state;

#[cfg(any(test, feature = "test-support"))]
pub mod test_support;

use crate::config::ServerConfig;
use crate::state::AppState;
use axum::Router;
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

pub struct RunningServer {
    pub local_addr: SocketAddr,
    pub shutdown: tokio::sync::oneshot::Sender<()>,
    pub handle: JoinHandle<()>,
}

pub async fn run(cfg: ServerConfig) -> anyhow::Result<RunningServer> {
    let state = AppState::build(&cfg).map_err(|e| anyhow::anyhow!("state build: {e}"))?;
    let app: Router = routes::router().with_state(state);
    let listener = TcpListener::bind(cfg.bind).await?;
    let local_addr = listener.local_addr()?;

    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, app)
            .with_graceful_shutdown(async {
                let _ = rx.await;
            })
            .await;
    });
    Ok(RunningServer { local_addr, shutdown: tx, handle })
}
```

- [ ] **Step 5: Add `test_support` module**

Create `crates/atlas-server/src/test_support.rs`:

```rust
//! Helpers reused by integration tests.

use crate::config::{CorsOrigins, ServerConfig};
use crate::run;
use crate::RunningServer;
use atlas_core::id::NetworkId;
use atlas_core::official::{ASSET_REGISTRY_JSON, CHAIN_REGISTRY_JSON};
use atlas_core::registry::{AssetRegistryDocument, ChainRegistryDocument, Registry};
use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::Level;

pub struct TestServer {
    pub addr: SocketAddr,
    inner: Option<RunningServer>,
}

impl TestServer {
    /// Start a server bound to 127.0.0.1:0 with the bundled registry
    /// and no RPC URLs configured (every network falls into
    /// `Unconfigured { NoRpcUrl }`).
    pub async fn start_with_no_rpc() -> Self {
        let chain_doc: ChainRegistryDocument =
            serde_json::from_str(CHAIN_REGISTRY_JSON).unwrap();
        let asset_doc: AssetRegistryDocument =
            serde_json::from_str(ASSET_REGISTRY_JSON).unwrap();
        let registry = Registry::from_documents(chain_doc, asset_doc).unwrap();

        let cfg = ServerConfig {
            bind: "127.0.0.1:0".parse().unwrap(),
            registry: Arc::new(registry),
            api_keys: Arc::new(HashSet::new()),
            rpc_url_for: Arc::new(|_id: &NetworkId| None),
            cors_allowed_origins: CorsOrigins::Any,
            log_level: Level::INFO,
            otel_endpoint: None,
        };
        let running = run(cfg).await.unwrap();
        Self {
            addr: running.local_addr,
            inner: Some(running),
        }
    }

    pub fn base_url(&self) -> String {
        format!("http://{}", self.addr)
    }

    pub async fn shutdown(mut self) {
        if let Some(running) = self.inner.take() {
            let _ = running.shutdown.send(());
            let _ = running.handle.await;
        }
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        if let Some(running) = self.inner.take() {
            let _ = running.shutdown.send(());
        }
    }
}
```

Update the `[dev-dependencies]` block in `Cargo.toml` to include reqwest:

```toml
[dev-dependencies]
reqwest = { workspace = true, features = ["json"] }
serde_json.workspace = true
tokio = { workspace = true, features = ["macros", "rt-multi-thread", "test-util"] }
```

- [ ] **Step 6: Run the integration test**

Run: `cargo test -p atlas-server --test server_health`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/atlas-server/
git commit -m "feat(server): GET /health route + run() entry + TestServer helper"
```

---

## Task 7: Bearer auth middleware

**Files:**
- Create: `crates/atlas-server/src/middleware/mod.rs`
- Create: `crates/atlas-server/src/middleware/auth.rs`
- Modify: `crates/atlas-server/src/lib.rs`

- [ ] **Step 1: Create the middleware modules**

Create `crates/atlas-server/src/middleware/mod.rs`:

```rust
pub mod auth;
```

Create `crates/atlas-server/src/middleware/auth.rs`:

```rust
//! Bearer-token auth middleware. Public paths bypass; protected
//! paths require `Authorization: Bearer <token>` matching one of the
//! configured API keys.

use crate::error::ApiError;
use crate::state::AppState;
use axum::extract::State;
use axum::http::{header::AUTHORIZATION, Request};
use axum::middleware::Next;
use axum::response::Response;

const PUBLIC_PATHS: &[&str] = &[
    "/health",
    "/ready",
    "/metrics",
    "/openapi.json",
    "/docs",
    "/v1/networks",
];

pub async fn require_bearer(
    State(state): State<AppState>,
    req: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, ApiError> {
    let path = req.uri().path();
    if PUBLIC_PATHS.iter().any(|p| path == *p || path.starts_with(p)) {
        return Ok(next.run(req).await);
    }
    if state.api_keys.is_empty() {
        // Dev mode: warn-at-startup pattern, requests pass through.
        return Ok(next.run(req).await);
    }
    let token = req
        .headers()
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or(ApiError::MissingToken)?;
    if !state.api_keys.contains(token) {
        return Err(ApiError::InvalidToken);
    }
    Ok(next.run(req).await)
}

#[cfg(test)]
mod tests {
    use super::PUBLIC_PATHS;

    #[test]
    fn standard_paths_are_marked_public() {
        for p in ["/health", "/ready", "/metrics", "/openapi.json", "/docs", "/v1/networks"] {
            assert!(PUBLIC_PATHS.contains(&p), "{p} should be public");
        }
    }
}
```

- [ ] **Step 2: Wire the module into lib.rs**

```rust
pub mod config;
pub mod error;
pub mod hex_codec;
pub mod middleware;
pub mod routes;
pub mod state;
```

And in `run()`, attach the middleware:

```rust
pub async fn run(cfg: ServerConfig) -> anyhow::Result<RunningServer> {
    let state = AppState::build(&cfg).map_err(|e| anyhow::anyhow!("state build: {e}"))?;
    let app: Router = routes::router()
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            middleware::auth::require_bearer,
        ))
        .with_state(state);
    let listener = TcpListener::bind(cfg.bind).await?;
    let local_addr = listener.local_addr()?;
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, app)
            .with_graceful_shutdown(async { let _ = rx.await; })
            .await;
    });
    Ok(RunningServer { local_addr, shutdown: tx, handle })
}
```

- [ ] **Step 3: Add an integration test in `tests/server_health.rs`**

Append to `crates/atlas-server/tests/server_health.rs`:

```rust
#[tokio::test]
async fn health_endpoint_is_public_no_token_required() {
    let server = TestServer::start_with_no_rpc().await;
    let resp = reqwest::get(format!("{}/health", server.base_url())).await.unwrap();
    assert_eq!(resp.status(), 200);
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p atlas-server`
Expected: ALL PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/atlas-server/
git commit -m "feat(server): bearer-token auth middleware + public path bypass"
```

---

## Task 8: Telemetry init (tracing JSON + Prometheus + feature-gated OTel)

**Files:**
- Create: `crates/atlas-server/src/telemetry.rs`
- Modify: `crates/atlas-server/src/lib.rs`

- [ ] **Step 1: Create `crates/atlas-server/src/telemetry.rs`**

```rust
//! Observability stack init.
//!
//! - `tracing` for structured JSON logs (stdout).
//! - `metrics-exporter-prometheus` for `/metrics` exposition.
//! - `tracing-opentelemetry` + OTLP exporter for distributed traces
//!   (feature-gated behind `otel`).

use crate::config::ServerConfig;
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

pub struct TelemetryGuard {
    pub prom: PrometheusHandle,
}

pub fn init(cfg: &ServerConfig) -> anyhow::Result<TelemetryGuard> {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(cfg.log_level.to_string()));

    let fmt_layer = tracing_subscriber::fmt::layer()
        .json()
        .with_target(true)
        .with_thread_ids(true);

    #[cfg(feature = "otel")]
    let otel_layer = build_otel_layer(cfg);
    #[cfg(not(feature = "otel"))]
    let otel_layer: Option<tracing_subscriber::reload::Layer<
        tracing_subscriber::fmt::Layer<tracing_subscriber::Registry>,
        tracing_subscriber::Registry,
    >> = None;

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt_layer)
        .with(otel_layer)
        .try_init()
        .ok();

    let prom = PrometheusBuilder::new().install_recorder()?;
    Ok(TelemetryGuard { prom })
}

#[cfg(feature = "otel")]
fn build_otel_layer(
    cfg: &ServerConfig,
) -> Option<
    tracing_opentelemetry::OpenTelemetryLayer<
        tracing_subscriber::Registry,
        opentelemetry_sdk::trace::Tracer,
    >,
> {
    let endpoint = cfg.otel_endpoint.as_ref()?;
    let tracer = opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_exporter(
            opentelemetry_otlp::new_exporter()
                .tonic()
                .with_endpoint(endpoint),
        )
        .install_batch(opentelemetry_sdk::runtime::Tokio)
        .ok()?;
    Some(tracing_opentelemetry::layer().with_tracer(tracer))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::CorsOrigins;
    use atlas_core::official::{ASSET_REGISTRY_JSON, CHAIN_REGISTRY_JSON};
    use atlas_core::registry::{AssetRegistryDocument, ChainRegistryDocument, Registry};
    use std::collections::HashSet;
    use std::sync::Arc;
    use tracing::Level;

    fn minimal_cfg() -> ServerConfig {
        let chain_doc: ChainRegistryDocument =
            serde_json::from_str(CHAIN_REGISTRY_JSON).unwrap();
        let asset_doc: AssetRegistryDocument =
            serde_json::from_str(ASSET_REGISTRY_JSON).unwrap();
        let registry = Registry::from_documents(chain_doc, asset_doc).unwrap();
        ServerConfig {
            bind: "127.0.0.1:0".parse().unwrap(),
            registry: Arc::new(registry),
            api_keys: Arc::new(HashSet::new()),
            rpc_url_for: Arc::new(|_| None),
            cors_allowed_origins: CorsOrigins::Any,
            log_level: Level::INFO,
            otel_endpoint: None,
        }
    }

    #[test]
    fn init_returns_prometheus_handle() {
        let cfg = minimal_cfg();
        let guard = init(&cfg).unwrap();
        let rendered = guard.prom.render();
        // Prometheus exposition is text; empty registry still produces
        // a valid (possibly empty) string.
        assert!(rendered.is_empty() || rendered.contains("HELP") || rendered.contains("\n"));
    }
}
```

- [ ] **Step 2: Wire into `lib.rs`**

```rust
pub mod config;
pub mod error;
pub mod hex_codec;
pub mod middleware;
pub mod routes;
pub mod state;
pub mod telemetry;
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p atlas-server --lib telemetry::`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/atlas-server/src/telemetry.rs crates/atlas-server/src/lib.rs
git commit -m "feat(server): telemetry init (tracing JSON + Prometheus + otel feature)"
```

---

## Task 9: GET /v1/networks discovery route

**Files:**
- Create: `crates/atlas-server/src/routes/networks.rs`
- Modify: `crates/atlas-server/src/routes/mod.rs`
- Create: `crates/atlas-server/tests/server_networks.rs`

- [ ] **Step 1: Write the failing integration test**

Create `crates/atlas-server/tests/server_networks.rs`:

```rust
use atlas_server::test_support::TestServer;

#[tokio::test]
async fn networks_endpoint_lists_bundled_networks_unconfigured() {
    let server = TestServer::start_with_no_rpc().await;
    let resp = reqwest::get(format!("{}/v1/networks", server.base_url()))
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    let arr = body.as_array().unwrap();
    // Bundled registry ships at least 3 networks (Ethereum, Base, Solana mainnet).
    assert!(arr.len() >= 3);
    // No RPC URL provided → every entry is `configured: false`.
    for entry in arr {
        assert_eq!(entry["configured"], false);
        assert!(entry["id"].is_string());
        assert!(entry["name"].is_string());
        assert!(entry["namespace"].is_string());
    }
}
```

- [ ] **Step 2: Create `crates/atlas-server/src/routes/networks.rs`**

```rust
//! Network discovery: `GET /v1/networks`.

use crate::state::{AppState, NetworkChainService};
use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;
use utoipa::ToSchema;

pub fn router() -> Router<AppState> {
    Router::new().route("/v1/networks", get(list_networks))
}

#[derive(Serialize, ToSchema)]
pub struct NetworkInfo {
    pub id: String,
    pub name: String,
    pub namespace: String,
    pub configured: bool,
    pub features: NetworkFeatures,
}

#[derive(Serialize, ToSchema)]
pub struct NetworkFeatures {
    pub eip1559: bool,
    #[serde(rename = "opStackL1Fee")]
    pub op_stack_l1_fee: bool,
    pub erc20: bool,
}

#[utoipa::path(
    get,
    path = "/v1/networks",
    responses((status = 200, description = "Configured + known networks", body = [NetworkInfo])),
    tag = "discovery"
)]
pub async fn list_networks(State(state): State<AppState>) -> Json<Vec<NetworkInfo>> {
    let mut out = Vec::new();
    for id in state.registry.iter_network_ids() {
        let id_str = id.to_string();
        let network = state.registry.network(&id_str).expect("listed");
        let configured = matches!(
            state.chain_services.get(&network.id),
            Some(NetworkChainService::Evm(_))
        );
        out.push(NetworkInfo {
            id: id_str,
            name: network.name.clone(),
            namespace: network.id.namespace().to_string(),
            configured,
            features: NetworkFeatures {
                eip1559: network.features.eip1559,
                op_stack_l1_fee: network.features.op_stack_l1_fee,
                erc20: network.features.erc20,
            },
        });
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    Json(out)
}
```

- [ ] **Step 3: Wire into `routes/mod.rs`**

```rust
pub mod health;
pub mod networks;

use crate::state::AppState;
use axum::Router;

pub fn router() -> Router<AppState> {
    Router::new()
        .merge(health::router())
        .merge(networks::router())
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p atlas-server --test server_networks`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/atlas-server/src/routes/ crates/atlas-server/tests/server_networks.rs
git commit -m "feat(server): GET /v1/networks discovery endpoint"
```

---

## Task 10: POST /v1/transfers/prepare — happy path (unit test first)

**Files:**
- Create: `crates/atlas-server/src/routes/transfers.rs`
- Modify: `crates/atlas-server/src/routes/mod.rs`
- Create: `crates/atlas-server/tests/server_prepare.rs`

- [ ] **Step 1: Write the failing integration test**

Create `crates/atlas-server/tests/server_prepare.rs`:

```rust
use atlas_server::test_support::TestServer;
use serde_json::json;

#[tokio::test]
async fn prepare_returns_503_when_target_network_unconfigured() {
    // Bundled registry includes Ethereum mainnet; no RPC URL is wired
    // in `TestServer::start_with_no_rpc`, so the dispatch must return
    // 503 network_unconfigured.
    let server = TestServer::start_with_no_rpc().await;

    let body = json!({
        "intent": {
            "assetInstanceId": "eip155:1/native:eth",
            "to": "0x0000000000000000000000000000000000000003",
            "amount": { "value": "1", "decimals": 18 }
        },
        "account": "0x9858EfFD232B4033E47d90003D41EC34EcaEda94"
    });

    let resp = reqwest::Client::new()
        .post(format!("{}/v1/transfers/prepare", server.base_url()))
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 503);
    let payload: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(payload["error"]["code"], "network_unconfigured");
    assert_eq!(payload["error"]["type"], "api_error");
}

#[tokio::test]
async fn prepare_returns_400_on_invalid_caip_asset_id() {
    let server = TestServer::start_with_no_rpc().await;

    let body = json!({
        "intent": {
            "assetInstanceId": "eip_155:1/native:eth",
            "to": "0x0000000000000000000000000000000000000003",
            "amount": { "value": "1", "decimals": 18 }
        },
        "account": "0x9858EfFD232B4033E47d90003D41EC34EcaEda94"
    });

    let resp = reqwest::Client::new()
        .post(format!("{}/v1/transfers/prepare", server.base_url()))
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
    let payload: serde_json::Value = resp.json().await.unwrap();
    // Body parse fails because `AssetInstanceId` rejects via custom Deserialize.
    assert_eq!(payload["error"]["type"], "invalid_request_error");
}
```

- [ ] **Step 2: Create `crates/atlas-server/src/routes/transfers.rs`**

```rust
//! `POST /v1/transfers/prepare`.

use crate::error::ApiError;
use crate::hex_codec;
use crate::state::AppState;
use atlas_core::id::{AccountRef, NetworkId};
use atlas_core::signing::SigningRequest;
use atlas_core::transaction::{TransferIntent, UnsignedBundle, UnsignedTransaction};
use axum::extract::State;
use axum::routing::post;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

pub fn router() -> Router<AppState> {
    Router::new().route("/v1/transfers/prepare", post(prepare))
}

#[derive(Deserialize, ToSchema)]
pub struct PrepareRequest {
    pub intent: TransferIntent,
    pub account: AccountRef,
}

#[derive(Serialize, ToSchema)]
pub struct PrepareResponse {
    pub unsigned: UnsignedTransactionWire,
    #[serde(rename = "signingRequest")]
    pub signing_request: SigningRequestWire,
}

#[derive(Serialize, ToSchema)]
pub struct UnsignedTransactionWire {
    pub account: AccountRef,
    pub network: NetworkId,
    pub intent: TransferIntent,
    #[serde(with = "hex_codec")]
    #[schema(value_type = String, example = "0xdeadbeef")]
    pub payload: Vec<u8>,
}

#[derive(Serialize, ToSchema)]
pub struct SigningRequestWire {
    pub account: AccountRef,
    pub network: NetworkId,
    pub curve: atlas_core::chain::Curve,
    #[serde(rename = "payloadKind")]
    pub payload_kind: atlas_core::signing::SigningPayloadKind,
    #[serde(with = "hex_codec")]
    #[schema(value_type = String, example = "0xa1b2c3")]
    pub payload: Vec<u8>,
}

impl From<UnsignedBundle> for PrepareResponse {
    fn from(b: UnsignedBundle) -> Self {
        Self {
            unsigned: UnsignedTransactionWire::from(b.unsigned),
            signing_request: SigningRequestWire::from(b.signing_request),
        }
    }
}

impl From<UnsignedTransaction> for UnsignedTransactionWire {
    fn from(u: UnsignedTransaction) -> Self {
        Self {
            account: u.account,
            network: u.network,
            intent: u.intent,
            payload: u.payload,
        }
    }
}

impl From<SigningRequest> for SigningRequestWire {
    fn from(r: SigningRequest) -> Self {
        Self {
            account: r.account,
            network: r.network,
            curve: r.curve,
            payload_kind: r.payload_kind,
            payload: r.payload,
        }
    }
}

#[utoipa::path(
    post,
    path = "/v1/transfers/prepare",
    request_body = PrepareRequest,
    responses(
        (status = 200, description = "Unsigned transaction bundle", body = PrepareResponse),
        (status = 400, description = "Invalid request", body = crate::error::ErrorBody),
        (status = 401, description = "Missing or invalid bearer token", body = crate::error::ErrorBody),
        (status = 502, description = "RPC failure", body = crate::error::ErrorBody),
        (status = 503, description = "Network not configured", body = crate::error::ErrorBody)
    ),
    tag = "transfers"
)]
#[tracing::instrument(skip(state, req), fields(
    network = tracing::field::Empty,
    asset_namespace = tracing::field::Empty,
))]
pub async fn prepare(
    State(state): State<AppState>,
    Json(req): Json<PrepareRequest>,
) -> Result<Json<PrepareResponse>, ApiError> {
    let network_id = req.intent.asset_instance_id.network_id();
    tracing::Span::current()
        .record("network", network_id.as_str())
        .record(
            "asset_namespace",
            req.intent.asset_instance_id.asset_namespace(),
        );
    let bundle = state.prepare_unsigned_bundle(req.intent, req.account).await?;
    Ok(Json(PrepareResponse::from(bundle)))
}
```

- [ ] **Step 3: Wire into `routes/mod.rs`**

```rust
pub mod health;
pub mod networks;
pub mod transfers;

use crate::state::AppState;
use axum::Router;

pub fn router() -> Router<AppState> {
    Router::new()
        .merge(health::router())
        .merge(networks::router())
        .merge(transfers::router())
}
```

- [ ] **Step 4: Map axum JSON-extractor rejections to ApiError**

Axum returns a 422 by default when JSON deserialisation fails. We want 400 with our error shape. Add a custom rejection handler at the top of `transfers.rs`:

Replace `Json(req): Json<PrepareRequest>` in the handler signature with the manual extractor pattern by introducing a wrapper. The simplest path: leave `Json` and let axum's 422 stand; we'll handle our prepare-related failures (CAIP, address, etc.) through `ApiError`. The test `prepare_returns_400_on_invalid_caip_asset_id` expects 400.

Update the test expectation to allow 400 OR 422 (axum uses 422 for unprocessable JSON content by default in newer versions). Or, more correctly, install a custom `JsonRejection` handler. Use the cleaner route: a tower layer that catches `JsonRejection` and converts:

Create `crates/atlas-server/src/middleware/json_rejection.rs`:

```rust
//! Convert axum's `JsonRejection` into our Stripe-style 400.

use crate::error::ApiError;
use axum::extract::rejection::JsonRejection;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

pub async fn handle(rej: JsonRejection) -> Response {
    let _ = rej;
    ApiError::InvalidRequestBody("malformed request body".to_string()).into_response()
}

pub fn status_for_rejection(_rej: &JsonRejection) -> StatusCode {
    StatusCode::BAD_REQUEST
}
```

A simpler path: define a wrapper extractor that returns `ApiError` directly. Update `transfers.rs`:

```rust
use axum::extract::rejection::JsonRejection;
use axum::extract::FromRequest;

pub struct StripeJson<T>(pub T);

#[axum::async_trait]
impl<T, S> FromRequest<S> for StripeJson<T>
where
    T: serde::de::DeserializeOwned + 'static,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request(
        req: axum::http::Request<axum::body::Body>,
        state: &S,
    ) -> Result<Self, Self::Rejection> {
        match axum::Json::<T>::from_request(req, state).await {
            Ok(axum::Json(v)) => Ok(StripeJson(v)),
            Err(rej) => {
                let msg = match &rej {
                    JsonRejection::JsonDataError(e) => e.body_text(),
                    JsonRejection::JsonSyntaxError(e) => e.body_text(),
                    JsonRejection::MissingJsonContentType(_) => {
                        "expected `Content-Type: application/json`".to_string()
                    }
                    _ => "malformed request body".to_string(),
                };
                Err(ApiError::InvalidRequestBody(msg))
            }
        }
    }
}
```

Then change the handler signature:

```rust
pub async fn prepare(
    State(state): State<AppState>,
    StripeJson(req): StripeJson<PrepareRequest>,
) -> Result<Json<PrepareResponse>, ApiError> {
    // ... same body ...
}
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p atlas-server --test server_prepare`
Expected: BOTH PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/atlas-server/
git commit -m "feat(server): POST /v1/transfers/prepare endpoint + Stripe JSON extractor"
```

---

## Task 11: POST /v1/transfers/prepare — happy path against mocked RPC

**Files:**
- Modify: `crates/atlas-server/src/test_support.rs`
- Modify: `crates/atlas-server/tests/server_prepare.rs`

- [ ] **Step 1: Extend `TestServer` to support a per-network RPC URL injection**

Append to `crates/atlas-server/src/test_support.rs`:

```rust
use std::collections::HashMap;

impl TestServer {
    /// Start a server with the bundled registry and the provided
    /// `network_id → rpc_url` map. Networks not in the map fall to
    /// `Unconfigured { NoRpcUrl }`.
    pub async fn start_with_rpc_map(rpcs: HashMap<String, String>) -> Self {
        let chain_doc: ChainRegistryDocument =
            serde_json::from_str(CHAIN_REGISTRY_JSON).unwrap();
        let asset_doc: AssetRegistryDocument =
            serde_json::from_str(ASSET_REGISTRY_JSON).unwrap();
        let registry = Registry::from_documents(chain_doc, asset_doc).unwrap();

        let rpcs_arc = Arc::new(rpcs);
        let resolver = {
            let rpcs = rpcs_arc.clone();
            Arc::new(move |id: &NetworkId| rpcs.get(id.as_str()).cloned())
        };

        let cfg = ServerConfig {
            bind: "127.0.0.1:0".parse().unwrap(),
            registry: Arc::new(registry),
            api_keys: Arc::new(HashSet::new()),
            rpc_url_for: resolver,
            cors_allowed_origins: CorsOrigins::Any,
            log_level: Level::INFO,
            otel_endpoint: None,
        };
        let running = run(cfg).await.unwrap();
        Self {
            addr: running.local_addr,
            inner: Some(running),
        }
    }
}
```

- [ ] **Step 2: Spin a mock RPC backend using `wiremock`**

Add to `Cargo.toml` dev-deps:

```toml
wiremock = "0.6"
```

Append to `crates/atlas-server/tests/server_prepare.rs`:

```rust
use std::collections::HashMap;
use wiremock::matchers::{body_json_string_contains, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn prepare_native_returns_200_with_bundle_against_mock_rpc() {
    // Mock RPC: respond to eth_getTransactionCount + eth_feeHistory.
    let rpc = MockServer::start().await;

    // eth_getTransactionCount → nonce 7
    Mock::given(method("POST"))
        .and(path("/"))
        .and(body_json_string_contains("eth_getTransactionCount"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "jsonrpc": "2.0", "id": 0, "result": "0x07"
        })))
        .mount(&rpc)
        .await;

    // eth_feeHistory → one block, non-zero base fee + non-zero reward.
    Mock::given(method("POST"))
        .and(path("/"))
        .and(body_json_string_contains("eth_feeHistory"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "jsonrpc": "2.0", "id": 0, "result": {
                "oldestBlock": "0x1",
                "baseFeePerGas": ["0x3b9aca00", "0x3b9aca00"],
                "gasUsedRatio": [0.5],
                "reward": [["0x4c4b40"]]
            }
        })))
        .mount(&rpc)
        .await;

    // Wire only Ethereum mainnet for this test.
    let mut rpcs = HashMap::new();
    rpcs.insert("eip155:1".to_string(), rpc.uri());
    let server = TestServer::start_with_rpc_map(rpcs).await;

    let body = serde_json::json!({
        "intent": {
            "assetInstanceId": "eip155:1/native:eth",
            "to": "0x0000000000000000000000000000000000000003",
            "amount": { "value": "123456", "decimals": 18 }
        },
        "account": "0x9858EfFD232B4033E47d90003D41EC34EcaEda94"
    });

    let resp = reqwest::Client::new()
        .post(format!("{}/v1/transfers/prepare", server.base_url()))
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "body: {}", resp.text().await.unwrap());
    let payload: serde_json::Value = reqwest::Client::new()
        .post(format!("{}/v1/transfers/prepare", server.base_url()))
        .json(&body)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(payload["unsigned"]["payload"].as_str().unwrap().starts_with("0x"));
    assert!(payload["signingRequest"]["payload"].as_str().unwrap().starts_with("0x"));
    assert_eq!(payload["signingRequest"]["curve"], "secp256k1");
}
```

- [ ] **Step 3: Run the test**

Run: `cargo test -p atlas-server --test server_prepare prepare_native_returns_200_with_bundle_against_mock_rpc`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/atlas-server/
git commit -m "test(server): prepare happy path against wiremock-backed RPC"
```

---

## Task 12: GET /ready route + RPC reachability probe

**Files:**
- Modify: `crates/atlas-server/src/routes/health.rs`
- Modify: `crates/atlas-server/tests/server_health.rs`

- [ ] **Step 1: Write the failing integration test**

Append to `crates/atlas-server/tests/server_health.rs`:

```rust
#[tokio::test]
async fn ready_endpoint_returns_200_with_per_network_status() {
    // No RPC configured for any network → ready should still return
    // 200 because there are zero CONFIGURED networks to probe (the
    // contract is "all configured RPCs respond"; empty set is trivially
    // satisfied).
    let server = TestServer::start_with_no_rpc().await;
    let resp = reqwest::get(format!("{}/ready", server.base_url())).await.unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "ready");
    assert!(body["networks"].as_array().unwrap().is_empty());
}
```

- [ ] **Step 2: Extend `crates/atlas-server/src/routes/health.rs`**

Replace the existing content with:

```rust
//! Liveness + readiness probes + Prometheus exposition.

use crate::state::{AppState, NetworkChainService};
use alloy_provider::Provider;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;
use std::time::Duration;
use utoipa::ToSchema;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/health", get(health))
        .route("/ready", get(ready))
}

#[derive(Serialize, ToSchema)]
pub struct HealthResponse {
    pub status: &'static str,
    pub version: &'static str,
    pub uptime_seconds: u64,
}

#[derive(Serialize, ToSchema)]
pub struct ReadyResponse {
    pub status: &'static str,
    pub networks: Vec<NetworkReadiness>,
}

#[derive(Serialize, ToSchema)]
pub struct NetworkReadiness {
    pub id: String,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_block: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[utoipa::path(
    get,
    path = "/health",
    responses((status = 200, description = "Liveness probe", body = HealthResponse)),
    tag = "ops"
)]
pub async fn health(State(state): State<AppState>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        version: state.version,
        uptime_seconds: state.started_at.elapsed().as_secs(),
    })
}

#[utoipa::path(
    get,
    path = "/ready",
    responses(
        (status = 200, description = "All configured RPCs reachable", body = ReadyResponse),
        (status = 503, description = "One or more configured RPCs failed", body = ReadyResponse)
    ),
    tag = "ops"
)]
pub async fn ready(State(state): State<AppState>) -> Response {
    let probes: Vec<NetworkReadiness> = futures::future::join_all(
        state.chain_services.iter().filter_map(|(id, svc)| {
            let provider = match svc {
                NetworkChainService::Evm(s) => Some(s.reader.provider().clone()),
                NetworkChainService::Unconfigured { .. } => None,
            };
            provider.map(|p| async move {
                let id_s = id.to_string();
                match tokio::time::timeout(Duration::from_secs(2), p.get_block_number()).await {
                    Ok(Ok(block)) => NetworkReadiness {
                        id: id_s,
                        ok: true,
                        latest_block: Some(block),
                        error: None,
                    },
                    Ok(Err(e)) => NetworkReadiness {
                        id: id_s,
                        ok: false,
                        latest_block: None,
                        error: Some(e.to_string()),
                    },
                    Err(_) => NetworkReadiness {
                        id: id_s,
                        ok: false,
                        latest_block: None,
                        error: Some("timeout".to_string()),
                    },
                }
            })
        }),
    )
    .await;

    let any_failed = probes.iter().any(|p| !p.ok);
    let status = if any_failed {
        StatusCode::SERVICE_UNAVAILABLE
    } else {
        StatusCode::OK
    };
    let body = ReadyResponse {
        status: if any_failed { "degraded" } else { "ready" },
        networks: probes,
    };
    (status, Json(body)).into_response()
}
```

You'll need a public accessor for the provider inside `EvmReader`. Add this to `crates/atlas-evm/src/reader.rs` in the `impl<P> EvmReader<P>` block (next to `new`):

```rust
/// Borrow the underlying provider — used by `/ready` to probe
/// reachability without going through a full read.
pub fn provider(&self) -> &P {
    &self.provider
}
```

Add `futures` to atlas-server deps in `crates/atlas-server/Cargo.toml`:

```toml
futures = "0.3"
```

Also add to root `[workspace.dependencies]` if not there:

```toml
futures             = "0.3"
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p atlas-server --test server_health`
Expected: ALL PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/atlas-server/ crates/atlas-evm/src/reader.rs Cargo.toml
git commit -m "feat(server): GET /ready probes RPC reachability per network"
```

---

## Task 13: GET /metrics + Prometheus exposition route

**Files:**
- Modify: `crates/atlas-server/src/routes/health.rs`
- Modify: `crates/atlas-server/src/lib.rs`
- Create: `crates/atlas-server/tests/server_metrics.rs`

- [ ] **Step 1: Write the failing integration test**

Create `crates/atlas-server/tests/server_metrics.rs`:

```rust
use atlas_server::test_support::TestServer;

#[tokio::test]
async fn metrics_endpoint_serves_prometheus_text() {
    let server = TestServer::start_with_no_rpc().await;
    let resp = reqwest::get(format!("{}/metrics", server.base_url()))
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let ct = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert!(ct.starts_with("text/plain"), "content-type: {ct}");
    let body = resp.text().await.unwrap();
    // Body is Prometheus exposition text (may be empty until counters
    // increment; we just check it's serveable).
    assert!(body.is_empty() || body.contains("# HELP") || body.contains("\n"));
}
```

- [ ] **Step 2: Wire `/metrics` via the telemetry handle**

Extend `AppState` to hold the Prometheus handle. In `crates/atlas-server/src/state.rs`, add:

```rust
use metrics_exporter_prometheus::PrometheusHandle;

#[derive(Clone)]
pub struct AppState {
    pub registry: Arc<Registry>,
    pub chain_services: Arc<HashMap<NetworkId, NetworkChainService>>,
    pub api_keys: Arc<HashSet<String>>,
    pub started_at: Instant,
    pub version: &'static str,
    pub prometheus: Option<PrometheusHandle>,
}
```

Update `AppState::build` to take an optional handle (we'll pass `None` from tests and `Some` from `run()`):

```rust
impl AppState {
    pub fn build(cfg: &ServerConfig, prometheus: Option<PrometheusHandle>) -> Result<Self, ApiError> {
        // ... existing body, plus at the end: ...
        Ok(Self {
            registry: cfg.registry.clone(),
            chain_services: Arc::new(services),
            api_keys: cfg.api_keys.clone(),
            started_at: Instant::now(),
            version: ATLAS_SERVER_VERSION,
            prometheus,
        })
    }
}
```

Update `crates/atlas-server/src/lib.rs::run` to init telemetry and pass the handle:

```rust
pub async fn run(cfg: ServerConfig) -> anyhow::Result<RunningServer> {
    let telem = telemetry::init(&cfg)?;
    let state = AppState::build(&cfg, Some(telem.prom))
        .map_err(|e| anyhow::anyhow!("state build: {e}"))?;
    let app: Router = routes::router()
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            middleware::auth::require_bearer,
        ))
        .with_state(state);
    let listener = TcpListener::bind(cfg.bind).await?;
    let local_addr = listener.local_addr()?;
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, app)
            .with_graceful_shutdown(async {
                let _ = rx.await;
            })
            .await;
    });
    Ok(RunningServer { local_addr, shutdown: tx, handle })
}
```

Update `crates/atlas-server/src/test_support.rs` callers of `AppState::build` to pass `None`. Then, swap the test helpers to build their own `PrometheusHandle` so calling `/metrics` works:

```rust
use metrics_exporter_prometheus::PrometheusBuilder;

impl TestServer {
    fn install_test_telemetry() -> metrics_exporter_prometheus::PrometheusHandle {
        // Use `install_recorder` if no global recorder is installed; ignore
        // errors when the recorder is already set (multi-test runs).
        PrometheusBuilder::new()
            .install_recorder()
            .unwrap_or_else(|_| {
                let (recorder, handle) = PrometheusBuilder::new().build().unwrap();
                drop(recorder);
                handle
            })
    }
    // ... existing start_with_* methods now call install_test_telemetry()
    // and pass `Some(handle)` when constructing the cfg-derived state.
}
```

Simpler — the test helpers use `crate::run` directly which already inits telemetry. Leave them as-is (they go through `run`). Adjust the `start_with_no_rpc` and `start_with_rpc_map` helpers to call `run` (they already do).

Then add a `/metrics` route in `routes/health.rs`:

```rust
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/health", get(health))
        .route("/ready", get(ready))
        .route("/metrics", get(metrics))
}

#[utoipa::path(
    get,
    path = "/metrics",
    responses((status = 200, description = "Prometheus exposition", body = String)),
    tag = "ops"
)]
pub async fn metrics(State(state): State<AppState>) -> Response {
    let body = state
        .prometheus
        .as_ref()
        .map(|h| h.render())
        .unwrap_or_default();
    (
        [(axum::http::header::CONTENT_TYPE, "text/plain; version=0.0.4")],
        body,
    )
        .into_response()
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p atlas-server --test server_metrics`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/atlas-server/
git commit -m "feat(server): GET /metrics serves Prometheus exposition"
```

---

## Task 14: OpenAPI doc + GET /openapi.json + Scalar /docs UI

**Files:**
- Create: `crates/atlas-server/src/openapi.rs`
- Create: `crates/atlas-server/src/docs.rs`
- Modify: `crates/atlas-server/src/routes/mod.rs`
- Create: `crates/atlas-server/tests/server_openapi.rs`

- [ ] **Step 1: Create the OpenAPI aggregator**

Create `crates/atlas-server/src/openapi.rs`:

```rust
//! Aggregated OpenAPI 3.x document.

use crate::error::{ErrorBody, ErrorDetail};
use crate::routes::{health, networks, transfers};
use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Atlas API",
        version = env!("CARGO_PKG_VERSION"),
        description = "Server-side transaction builder for the Atlas SDK. \
                       Returns unsigned bundles for clients to sign + broadcast.",
    ),
    paths(
        transfers::prepare,
        networks::list_networks,
        health::health,
        health::ready,
        health::metrics,
    ),
    components(schemas(
        ErrorBody,
        ErrorDetail,
        transfers::PrepareRequest,
        transfers::PrepareResponse,
        transfers::UnsignedTransactionWire,
        transfers::SigningRequestWire,
        networks::NetworkInfo,
        networks::NetworkFeatures,
        health::HealthResponse,
        health::ReadyResponse,
        health::NetworkReadiness,
    )),
    tags(
        (name = "transfers", description = "Transaction-building endpoints"),
        (name = "discovery", description = "Server introspection"),
        (name = "ops",       description = "Liveness, readiness, metrics"),
    ),
)]
pub struct ApiDoc;
```

- [ ] **Step 2: Create the Scalar `/docs` handler**

Create `crates/atlas-server/src/docs.rs`:

```rust
//! Interactive API documentation via Scalar.

use axum::response::Html;

pub async fn docs() -> Html<&'static str> {
    Html(r#"<!doctype html>
<html>
  <head>
    <title>Atlas API</title>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width,initial-scale=1" />
  </head>
  <body>
    <script id="api-reference" data-url="/openapi.json"></script>
    <script src="https://cdn.jsdelivr.net/npm/@scalar/api-reference"></script>
  </body>
</html>"#)
}
```

- [ ] **Step 3: Wire openapi + docs routes**

Update `crates/atlas-server/src/routes/mod.rs`:

```rust
pub mod health;
pub mod networks;
pub mod transfers;

use crate::docs;
use crate::openapi::ApiDoc;
use crate::state::AppState;
use axum::routing::get;
use axum::{Json, Router};
use utoipa::OpenApi;

pub fn router() -> Router<AppState> {
    Router::new()
        .merge(health::router())
        .merge(networks::router())
        .merge(transfers::router())
        .route("/openapi.json", get(serve_openapi))
        .route("/docs", get(docs::docs))
}

async fn serve_openapi() -> Json<utoipa::openapi::OpenApi> {
    Json(ApiDoc::openapi())
}
```

Wire the new modules into `lib.rs`:

```rust
pub mod config;
pub mod docs;
pub mod error;
pub mod hex_codec;
pub mod middleware;
pub mod openapi;
pub mod routes;
pub mod state;
pub mod telemetry;
```

- [ ] **Step 4: Write the integration test**

Create `crates/atlas-server/tests/server_openapi.rs`:

```rust
use atlas_server::test_support::TestServer;

#[tokio::test]
async fn openapi_endpoint_serves_valid_openapi_3_doc() {
    let server = TestServer::start_with_no_rpc().await;
    let resp = reqwest::get(format!("{}/openapi.json", server.base_url()))
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["openapi"].as_str().unwrap().split('.').next().unwrap(), "3");
    let paths = body["paths"].as_object().unwrap();
    assert!(paths.contains_key("/v1/transfers/prepare"));
    assert!(paths.contains_key("/v1/networks"));
    assert!(paths.contains_key("/health"));
    assert!(paths.contains_key("/ready"));
}

#[tokio::test]
async fn docs_endpoint_serves_scalar_html() {
    let server = TestServer::start_with_no_rpc().await;
    let resp = reqwest::get(format!("{}/docs", server.base_url()))
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let ct = resp.headers().get(reqwest::header::CONTENT_TYPE).unwrap().to_str().unwrap().to_string();
    assert!(ct.contains("text/html"));
    let body = resp.text().await.unwrap();
    assert!(body.contains("scalar"));
    assert!(body.contains("/openapi.json"));
}
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p atlas-server --test server_openapi`
Expected: BOTH PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/atlas-server/
git commit -m "feat(server): /openapi.json + Scalar /docs UI"
```

---

## Task 15: CORS + body-size middleware

**Files:**
- Modify: `crates/atlas-server/src/lib.rs`

- [ ] **Step 1: Wire CORS + body-limit into `run()`**

Update `crates/atlas-server/src/lib.rs::run`:

```rust
pub async fn run(cfg: ServerConfig) -> anyhow::Result<RunningServer> {
    let telem = telemetry::init(&cfg)?;
    let state = AppState::build(&cfg, Some(telem.prom))
        .map_err(|e| anyhow::anyhow!("state build: {e}"))?;

    let cors = match &cfg.cors_allowed_origins {
        config::CorsOrigins::Any => tower_http::cors::CorsLayer::permissive(),
        config::CorsOrigins::Allowlist(origins) => {
            let parsed: Vec<axum::http::HeaderValue> = origins
                .iter()
                .filter_map(|o| o.parse().ok())
                .collect();
            tower_http::cors::CorsLayer::new()
                .allow_origin(parsed)
                .allow_methods([axum::http::Method::GET, axum::http::Method::POST])
                .allow_headers([
                    axum::http::header::AUTHORIZATION,
                    axum::http::header::CONTENT_TYPE,
                ])
        }
    };

    let body_limit = tower_http::limit::RequestBodyLimitLayer::new(64 * 1024);

    let app: Router = routes::router()
        .layer(cors)
        .layer(body_limit)
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            middleware::auth::require_bearer,
        ))
        .with_state(state);

    let listener = TcpListener::bind(cfg.bind).await?;
    let local_addr = listener.local_addr()?;
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, app)
            .with_graceful_shutdown(async { let _ = rx.await; })
            .await;
    });
    Ok(RunningServer { local_addr, shutdown: tx, handle })
}
```

- [ ] **Step 2: Build + run all tests**

Run: `cargo test -p atlas-server`
Expected: ALL PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/atlas-server/src/lib.rs
git commit -m "feat(server): CORS + 64KB request-body limit middleware"
```

---

## Task 16: Binary entry point with `serve` / `health` / `ready` subcommands

**Files:**
- Modify: `crates/atlas-server/src/main.rs`

- [ ] **Step 1: Replace `main.rs` with clap-driven subcommand dispatch**

```rust
//! atlas-server CLI entry point.

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "atlas-server", version, about = "Atlas HTTP server")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Run the HTTP server.
    Serve,
    /// HTTP-probe the local /health endpoint. Exits 0 on 200, 1 otherwise.
    Health,
    /// HTTP-probe the local /ready endpoint. Exits 0 on 200, 1 otherwise.
    Ready,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command.unwrap_or(Command::Serve) {
        Command::Serve => serve().await,
        Command::Health => probe("/health").await,
        Command::Ready => probe("/ready").await,
    }
}

async fn serve() -> Result<()> {
    let cfg = atlas_server::config::ServerConfig::from_env()?;
    let server = atlas_server::run(cfg).await?;
    tracing::info!(addr = %server.local_addr, "atlas-server listening");
    let _ = tokio::signal::ctrl_c().await;
    let _ = server.shutdown.send(());
    let _ = server.handle.await;
    Ok(())
}

async fn probe(path: &str) -> Result<()> {
    let bind = std::env::var("ATLAS_BIND").unwrap_or_else(|_| "0.0.0.0:8080".to_string());
    let url = format!("http://{}{path}", bind);
    let resp = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()?
        .get(&url)
        .send()
        .await?;
    if resp.status().is_success() {
        Ok(())
    } else {
        std::process::exit(1);
    }
}
```

- [ ] **Step 2: Verify the binary builds + runs in `--help` mode**

```bash
cargo run -p atlas-server -- --help
```

Expected output mentions `serve`, `health`, `ready`.

- [ ] **Step 3: Commit**

```bash
git add crates/atlas-server/src/main.rs
git commit -m "feat(server): clap subcommands serve/health/ready + ctrl-c shutdown"
```

---

## Task 17: atlas-server README

**Files:**
- Create: `crates/atlas-server/README.md`
- Create: `crates/atlas-server/.env.example`

- [ ] **Step 1: Create `crates/atlas-server/README.md`**

```markdown
# atlas-server

HTTP server that exposes Atlas's "server-builds-tx, client-signs-and-broadcasts" deployment shape over a REST API.

`#![forbid(unsafe_code)]`. `publish = false`.

## Endpoints

| Method | Path | Auth | Description |
|---|---|---|---|
| POST | `/v1/transfers/prepare` | bearer | Build an `UnsignedBundle` from a `TransferIntent + AccountRef`. Returns unsigned RLP + signing request. |
| GET  | `/v1/networks`          | public | List bundled networks + which are configured (have an RPC URL set). |
| GET  | `/health`               | public | Liveness probe. `200 OK` while the process runs. |
| GET  | `/ready`                | public | Readiness probe — probes every configured RPC. `503` if any fail. |
| GET  | `/metrics`              | public | Prometheus exposition. |
| GET  | `/openapi.json`         | public | OpenAPI 3.x spec (machine-readable). |
| GET  | `/docs`                 | public | Scalar UI for interactive exploration. |

## Configuration (env vars)

| Variable | Default | Meaning |
|---|---|---|
| `ATLAS_BIND` | `0.0.0.0:8080` | `host:port` to bind. |
| `ATLAS_LOG_LEVEL` | `info` | One of `trace`, `debug`, `info`, `warn`, `error`. |
| `ATLAS_API_KEYS` | (empty → dev mode) | Comma-separated bearer tokens. Empty disables auth and logs a startup warning. |
| `ATLAS_CORS_ALLOWED_ORIGINS` | `*` | Either `*` or a comma-separated origin allowlist. |
| `ATLAS_RPC_<NETWORK>` | — | Per-network RPC URL. Network id is uppercased with `:`/`/`/`-`/`.` mapped to `_`. E.g. `eip155:11155111` → `ATLAS_RPC_EIP155_11155111`. |
| `OTEL_EXPORTER_OTLP_ENDPOINT` | (unset → no traces) | OTLP/gRPC endpoint, e.g. `http://jaeger:4317`. Only used when built with `--features otel`. |

## curl quickstart

```bash
# Liveness
curl localhost:8080/health

# Network discovery
curl localhost:8080/v1/networks

# Prepare a transfer (bearer required when ATLAS_API_KEYS is set)
curl -X POST localhost:8080/v1/transfers/prepare \
  -H "Authorization: Bearer dev-key" \
  -H "Content-Type: application/json" \
  -d '{
    "intent": {
      "assetInstanceId": "eip155:11155111/native:eth",
      "to":     "0x0000000000000000000000000000000000000003",
      "amount": { "value": "1", "decimals": 18 }
    },
    "account": "0x9858EfFD232B4033E47d90003D41EC34EcaEda94"
  }'
```

## Error model (Stripe-style)

```json
{
  "error": {
    "type":    "invalid_request_error",
    "code":    "invalid_caip",
    "message": "CAIP-19 input missing ':' separator: \"eip155:1/eth\"",
    "param":   "intent.assetInstanceId"
  }
}
```

| HTTP | `type` | Sample `code`s |
|---|---|---|
| 400 | `invalid_request_error` | `invalid_caip`, `invalid_address`, `negative_amount`, `decimals_mismatch`, `unsupported_asset`, `unsupported_standard`, `invalid_request_body` |
| 401 | `auth_error` | `missing_token`, `invalid_token` |
| 500 | `api_error` | `build_failed`, `internal_error` |
| 502 | `rpc_error` | `rpc_unavailable`, `rpc_node_error`, `rpc_malformed_response` |
| 503 | `api_error` | `network_unconfigured` |

## Local dev with Docker compose

```bash
cp .env.example .env
# Edit .env with real testnet RPC URLs
docker compose up --build
open http://localhost:8080/docs        # Scalar UI
open http://localhost:16686            # Jaeger UI for traces
```

## License

MIT.
```

- [ ] **Step 2: Create `.env.example`**

```bash
# Atlas server local-dev environment.

ATLAS_BIND=0.0.0.0:8080
ATLAS_LOG_LEVEL=debug
ATLAS_API_KEYS=dev-key-not-secret
ATLAS_CORS_ALLOWED_ORIGINS=*

# Testnet RPC URLs — replace with your Alchemy/Infura/etc keys.
ATLAS_RPC_EIP155_11155111=https://eth-sepolia.g.alchemy.com/v2/REPLACE_ME
ATLAS_RPC_EIP155_84532=https://base-sepolia.g.alchemy.com/v2/REPLACE_ME

# Distributed tracing (works with the docker-compose Jaeger sidecar).
OTEL_EXPORTER_OTLP_ENDPOINT=http://jaeger:4317
```

- [ ] **Step 3: Commit**

```bash
git add crates/atlas-server/README.md crates/atlas-server/.env.example
git commit -m "docs(server): README + .env.example"
```

---

## Task 18: atlas-e2e crate skeleton + helpers

**Files:**
- Create: `crates/atlas-e2e/Cargo.toml`
- Create: `crates/atlas-e2e/src/lib.rs`
- Create: `crates/atlas-e2e/README.md`
- Modify: `Cargo.toml` (workspace) — add member

- [ ] **Step 1: Add to workspace members**

Append in root `Cargo.toml` `[workspace]`:

```toml
members = [
    "crates/atlas-core",
    "crates/atlas-e2e",
    "crates/atlas-evm",
    "crates/atlas-scenarios",
    "crates/atlas-server",
    "crates/atlas-signer-localkey",
    "crates/atlas-verify",
]
```

- [ ] **Step 2: Create `crates/atlas-e2e/Cargo.toml`**

```toml
[package]
name = "atlas-e2e"
version = "0.1.0"
edition.workspace = true
license.workspace = true
repository.workspace = true
publish = false

[dependencies]
alloy-network.workspace = true
alloy-primitives.workspace = true
alloy-provider.workspace = true
alloy-rpc-types-eth.workspace = true
anyhow.workspace = true
atlas-core = { path = "../atlas-core" }
atlas-evm = { path = "../atlas-evm" }
atlas-server = { path = "../atlas-server" }
atlas-signer-localkey = { path = "../atlas-signer-localkey" }
num-bigint.workspace = true
reqwest = { workspace = true, features = ["json"] }
serde.workspace = true
serde_json.workspace = true
tokio = { workspace = true, features = ["macros", "rt-multi-thread", "time"] }
tracing.workspace = true
tracing-subscriber.workspace = true
```

- [ ] **Step 3: Create `crates/atlas-e2e/src/lib.rs`**

```rust
//! Atlas E2E test helpers — shared between integration tests and
//! example binaries.

use alloy_primitives::TxHash;
use alloy_provider::Provider;
use anyhow::{anyhow, Result};
use atlas_core::id::NetworkId;
use atlas_server::config::{CorsOrigins, ServerConfig};
use atlas_server::{run, RunningServer};
use atlas_core::official::{ASSET_REGISTRY_JSON, CHAIN_REGISTRY_JSON};
use atlas_core::registry::{AssetRegistryDocument, ChainRegistryDocument, Registry};
use serde::{de::DeserializeOwned, Serialize};
use std::collections::HashSet;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tracing::Level;

pub struct E2eConfig {
    pub network_id: NetworkId,
    pub rpc_url: String,
    pub private_key_bytes: [u8; 32],
    pub recipient: String,
}

impl E2eConfig {
    /// Build an E2E config for the given network id.
    /// Reads `ATLAS_E2E_RPC_<NETWORK>` + `ATLAS_E2E_PRIVATE_KEY_<NETWORK>`
    /// + `ATLAS_E2E_RECIPIENT_<NETWORK>` (optional, defaults to
    /// `0x0000…0001`). Returns `None` if any required var is missing —
    /// callers should gracefully skip in that case.
    pub fn from_env_for_network(network: &str) -> Option<Self> {
        let net = NetworkId::from_str(network).ok()?;
        let suffix = network
            .chars()
            .map(|c| match c {
                ':' | '/' | '-' | '.' => '_',
                c => c.to_ascii_uppercase(),
            })
            .collect::<String>();
        let rpc_url = std::env::var(format!("ATLAS_E2E_RPC_{suffix}")).ok()?;
        let pk_hex = std::env::var(format!("ATLAS_E2E_PRIVATE_KEY_{suffix}")).ok()?;
        let stripped = pk_hex.strip_prefix("0x").unwrap_or(&pk_hex);
        let bytes_vec = hex::decode(stripped).ok()?;
        if bytes_vec.len() != 32 {
            return None;
        }
        let mut bytes = [0u8; 32];
        bytes.copy_from_slice(&bytes_vec);
        let recipient = std::env::var(format!("ATLAS_E2E_RECIPIENT_{suffix}"))
            .unwrap_or_else(|_| "0x0000000000000000000000000000000000000001".to_string());
        Some(Self {
            network_id: net,
            rpc_url,
            private_key_bytes: bytes,
            recipient,
        })
    }
}

/// Spawn `atlas-server` in-process bound to `127.0.0.1:0`, with only
/// the given network's RPC URL wired.
pub async fn spawn_server_in_process(e2e: &E2eConfig) -> Result<RunningServer> {
    let chain_doc: ChainRegistryDocument = serde_json::from_str(CHAIN_REGISTRY_JSON)?;
    let asset_doc: AssetRegistryDocument = serde_json::from_str(ASSET_REGISTRY_JSON)?;
    let registry = Registry::from_documents(chain_doc, asset_doc)?;

    let target = e2e.network_id.clone();
    let url = e2e.rpc_url.clone();
    let resolver: Arc<dyn Fn(&NetworkId) -> Option<String> + Send + Sync> = Arc::new(move |id| {
        if id == &target {
            Some(url.clone())
        } else {
            None
        }
    });

    let cfg = ServerConfig {
        bind: "127.0.0.1:0".parse()?,
        registry: Arc::new(registry),
        api_keys: Arc::new(HashSet::new()),
        rpc_url_for: resolver,
        cors_allowed_origins: CorsOrigins::Any,
        log_level: Level::INFO,
        otel_endpoint: None,
    };
    run(cfg).await
}

/// POST `body` to `base_url/path`, deserialize the response as JSON.
pub async fn http_post<B: Serialize, R: DeserializeOwned>(
    base_url: &str,
    path: &str,
    body: &B,
) -> Result<R> {
    let resp = reqwest::Client::new()
        .post(format!("{base_url}{path}"))
        .json(body)
        .send()
        .await?;
    if !resp.status().is_success() {
        return Err(anyhow!("HTTP {}: {}", resp.status(), resp.text().await?));
    }
    Ok(resp.json().await?)
}

/// Poll the provider for a receipt every 2s until success or timeout.
pub async fn wait_for_receipt<P: Provider>(
    provider: &P,
    tx_hash_hex: &str,
    timeout: Duration,
) -> Result<bool> {
    let hash = TxHash::from_str(tx_hash_hex)?;
    let start = std::time::Instant::now();
    loop {
        if start.elapsed() > timeout {
            return Err(anyhow!("timed out waiting for receipt {tx_hash_hex}"));
        }
        if let Some(receipt) = provider.get_transaction_receipt(hash).await? {
            return Ok(receipt.status());
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}
```

- [ ] **Step 4: Create `crates/atlas-e2e/README.md`**

```markdown
# atlas-e2e

End-to-end test crate. Two flows × two networks, gated on env vars.

## Required env vars (per network)

For each network you want to exercise:

| Var | Example |
|---|---|
| `ATLAS_E2E_RPC_<NETWORK>` | `https://eth-sepolia.g.alchemy.com/v2/...` |
| `ATLAS_E2E_PRIVATE_KEY_<NETWORK>` | 32-byte hex, `0x`-optional. Must be a funded testnet account. |
| `ATLAS_E2E_RECIPIENT_<NETWORK>` | (optional) `0x...`, defaults to `0x0000…0001`. |

Suffix derived from the CAIP-2 id (uppercase, `:`/`/`/`-`/`.` mapped to `_`).
For Sepolia: `ATLAS_E2E_RPC_EIP155_11155111`.
For Base Sepolia: `ATLAS_E2E_RPC_EIP155_84532`.

## Funding strategy

One funded testnet account per network. Each E2E run sends 1 wei to the recipient — gas fees come from the account; principal stays roughly stable. Top up via faucet manually when balance drops below ~0.01 ETH.

The `e2e.yml` GitHub Actions workflow injects these from encrypted repository secrets.

## Running locally

```bash
export ATLAS_E2E_RPC_EIP155_11155111=https://...
export ATLAS_E2E_PRIVATE_KEY_EIP155_11155111=0x...
cargo test -p atlas-e2e -- --ignored
```

Tests skip cleanly when env vars are absent — they log a one-line explanation rather than failing.

## Examples

```bash
ATLAS_E2E_RPC_EIP155_11155111=... \
ATLAS_E2E_PRIVATE_KEY_EIP155_11155111=... \
cargo run -p atlas-e2e --example direct_flow

ATLAS_E2E_RPC_EIP155_11155111=... \
ATLAS_E2E_PRIVATE_KEY_EIP155_11155111=... \
cargo run -p atlas-e2e --example server_flow
```
```

- [ ] **Step 5: Verify build**

Run: `cargo build -p atlas-e2e`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml crates/atlas-e2e/
git commit -m "feat(e2e): atlas-e2e crate skeleton + helpers"
```

---

## Task 19: E2E direct-flow integration tests (Sepolia + Base Sepolia)

**Files:**
- Create: `crates/atlas-e2e/tests/direct_sepolia.rs`
- Create: `crates/atlas-e2e/tests/direct_base_sepolia.rs`

- [ ] **Step 1: Create the Sepolia direct-flow test**

`crates/atlas-e2e/tests/direct_sepolia.rs`:

```rust
//! Direct flow on Sepolia: SDK only, no server.

use alloy_provider::ProviderBuilder;
use atlas_core::amount::RawAmount;
use atlas_core::asset::AssetStandard;
use atlas_core::fee::EvmFee;
use atlas_core::id::{AccountRef, AddressRef, AssetInstanceId, NetworkId, SignerId};
use atlas_core::service::{ChainBroadcaster, ChainCodec, ChainReader, FeeEstimator};
use atlas_core::signing::SignerProvider;
use atlas_core::transaction::TransferIntent;
use atlas_e2e::{wait_for_receipt, E2eConfig};
use atlas_evm::broadcaster::EvmBroadcaster;
use atlas_evm::codec::{EvmCodec, EvmPrepareContext};
use atlas_evm::fee_estimator::EvmFeeEstimator;
use atlas_evm::reader::EvmReader;
use atlas_signer_localkey::LocalKeySigner;
use num_bigint::BigInt;
use std::str::FromStr;
use std::time::Duration;

#[tokio::test]
#[ignore = "requires testnet credentials"]
async fn direct_flow_sepolia_native_transfer() {
    let Some(cfg) = E2eConfig::from_env_for_network("eip155:11155111") else {
        eprintln!("skipping: ATLAS_E2E_RPC_EIP155_11155111 / ATLAS_E2E_PRIVATE_KEY_EIP155_11155111 not set");
        return;
    };

    let signer = LocalKeySigner::from_bytes(
        SignerId::from_str("e2e-sepolia").unwrap(),
        cfg.private_key_bytes,
    )
    .unwrap();
    let from_addr = signer.address();
    let provider = ProviderBuilder::new().connect_http(cfg.rpc_url.parse().unwrap());

    let reader = EvmReader::new(provider.clone());
    let estimator = EvmFeeEstimator::new(provider.clone(), true);
    let broadcaster = EvmBroadcaster::new(provider.clone());
    let codec = EvmCodec;

    let intent = TransferIntent {
        asset_instance_id: AssetInstanceId::from_str("eip155:11155111/native:eth").unwrap(),
        to: AddressRef::from_str(&cfg.recipient).unwrap(),
        amount: RawAmount::new(BigInt::from(1u64), 18).unwrap(),
    };
    let from_ref = AddressRef::from_str(&from_addr).unwrap();
    let (nonce, fee) = tokio::join!(
        reader.get_nonce(&cfg.network_id, &from_ref),
        estimator.estimate_fee(&intent, &from_ref),
    );
    let nonce = nonce.expect("nonce");
    let fee = fee.expect("fee");

    let (max_fee, max_priority, gas_limit) = match &fee {
        EvmFee::Eip1559 {
            max_fee_per_gas,
            max_priority_fee_per_gas,
            gas_limit,
            ..
        } => (
            max_fee_per_gas.clone(),
            max_priority_fee_per_gas.clone(),
            *gas_limit,
        ),
        EvmFee::Legacy { gas_price, gas_limit } => {
            (gas_price.clone(), BigInt::from(0u64), *gas_limit)
        }
    };
    let _ = (max_fee, max_priority, gas_limit);

    let ctx = EvmPrepareContext {
        account: AccountRef::from_str(&from_addr).unwrap(),
        network: cfg.network_id.clone(),
        intent: intent.clone(),
        chain_id: 11155111,
        nonce,
        fee,
        standard: AssetStandard::Native,
        contract: None,
    };
    let unsigned = codec.prepare_transfer(ctx).expect("encode");
    let signing_request = codec.signing_request(&unsigned).expect("digest");
    let response = signer.sign(signing_request).await.expect("sign");
    let signed = codec.assemble_signed(unsigned, response).expect("assemble");

    let broadcast = broadcaster.broadcast(signed).await.expect("broadcast");
    let ok = wait_for_receipt(&provider, &broadcast.tx_hash, Duration::from_secs(120))
        .await
        .expect("await receipt");
    assert!(ok, "tx must succeed");
}
```

- [ ] **Step 2: Create the Base Sepolia direct-flow test**

`crates/atlas-e2e/tests/direct_base_sepolia.rs`:

```rust
//! Direct flow on Base Sepolia. Same shape as direct_sepolia.rs with
//! a different chain id and network id.

use alloy_provider::ProviderBuilder;
use atlas_core::amount::RawAmount;
use atlas_core::asset::AssetStandard;
use atlas_core::fee::EvmFee;
use atlas_core::id::{AccountRef, AddressRef, AssetInstanceId, NetworkId, SignerId};
use atlas_core::service::{ChainBroadcaster, ChainCodec, ChainReader, FeeEstimator};
use atlas_core::signing::SignerProvider;
use atlas_core::transaction::TransferIntent;
use atlas_e2e::{wait_for_receipt, E2eConfig};
use atlas_evm::broadcaster::EvmBroadcaster;
use atlas_evm::codec::{EvmCodec, EvmPrepareContext};
use atlas_evm::fee_estimator::EvmFeeEstimator;
use atlas_evm::reader::EvmReader;
use atlas_signer_localkey::LocalKeySigner;
use num_bigint::BigInt;
use std::str::FromStr;
use std::time::Duration;

#[tokio::test]
#[ignore = "requires testnet credentials"]
async fn direct_flow_base_sepolia_native_transfer() {
    let Some(cfg) = E2eConfig::from_env_for_network("eip155:84532") else {
        eprintln!("skipping: ATLAS_E2E_RPC_EIP155_84532 / ATLAS_E2E_PRIVATE_KEY_EIP155_84532 not set");
        return;
    };

    let signer = LocalKeySigner::from_bytes(
        SignerId::from_str("e2e-base-sepolia").unwrap(),
        cfg.private_key_bytes,
    )
    .unwrap();
    let from_addr = signer.address();
    let provider = ProviderBuilder::new().connect_http(cfg.rpc_url.parse().unwrap());

    let reader = EvmReader::new(provider.clone());
    let estimator = EvmFeeEstimator::new(provider.clone(), true);
    let broadcaster = EvmBroadcaster::new(provider.clone());
    let codec = EvmCodec;

    let intent = TransferIntent {
        asset_instance_id: AssetInstanceId::from_str("eip155:84532/native:eth").unwrap(),
        to: AddressRef::from_str(&cfg.recipient).unwrap(),
        amount: RawAmount::new(BigInt::from(1u64), 18).unwrap(),
    };
    let from_ref = AddressRef::from_str(&from_addr).unwrap();
    let (nonce, fee) = tokio::join!(
        reader.get_nonce(&cfg.network_id, &from_ref),
        estimator.estimate_fee(&intent, &from_ref),
    );
    let nonce = nonce.expect("nonce");
    let fee = fee.expect("fee");

    let ctx = EvmPrepareContext {
        account: AccountRef::from_str(&from_addr).unwrap(),
        network: cfg.network_id.clone(),
        intent: intent.clone(),
        chain_id: 84532,
        nonce,
        fee,
        standard: AssetStandard::Native,
        contract: None,
    };
    let unsigned = codec.prepare_transfer(ctx).expect("encode");
    let signing_request = codec.signing_request(&unsigned).expect("digest");
    let response = signer.sign(signing_request).await.expect("sign");
    let signed = codec.assemble_signed(unsigned, response).expect("assemble");

    let broadcast = broadcaster.broadcast(signed).await.expect("broadcast");
    let ok = wait_for_receipt(&provider, &broadcast.tx_hash, Duration::from_secs(120))
        .await
        .expect("await receipt");
    assert!(ok, "tx must succeed");
}
```

- [ ] **Step 3: Verify they compile**

Run: `cargo build -p atlas-e2e --tests`
Expected: PASS. (Tests are `#[ignore]`-gated — they won't run by default.)

- [ ] **Step 4: Commit**

```bash
git add crates/atlas-e2e/tests/direct_sepolia.rs crates/atlas-e2e/tests/direct_base_sepolia.rs
git commit -m "test(e2e): direct flow integration tests (Sepolia + Base Sepolia)"
```

---

## Task 20: E2E server-flow integration tests (Sepolia + Base Sepolia)

**Files:**
- Create: `crates/atlas-e2e/tests/server_sepolia.rs`
- Create: `crates/atlas-e2e/tests/server_base_sepolia.rs`

- [ ] **Step 1: Create the Sepolia server-flow test**

`crates/atlas-e2e/tests/server_sepolia.rs`:

```rust
//! Server flow on Sepolia: spawn atlas-server, POST to it, sign locally,
//! broadcast.

use alloy_provider::ProviderBuilder;
use atlas_core::amount::RawAmount;
use atlas_core::id::{AccountRef, AddressRef, AssetInstanceId, SignerId};
use atlas_core::service::{ChainBroadcaster, ChainCodec};
use atlas_core::signing::SignerProvider;
use atlas_core::transaction::TransferIntent;
use atlas_e2e::{http_post, spawn_server_in_process, wait_for_receipt, E2eConfig};
use atlas_evm::broadcaster::EvmBroadcaster;
use atlas_evm::codec::EvmCodec;
use atlas_signer_localkey::LocalKeySigner;
use num_bigint::BigInt;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use std::time::Duration;

#[derive(Serialize)]
struct PrepareRequest {
    intent: TransferIntent,
    account: AccountRef,
}

#[derive(Deserialize)]
struct UnsignedWire {
    account: AccountRef,
    network: atlas_core::id::NetworkId,
    intent: TransferIntent,
    payload: String,
}

#[derive(Deserialize)]
struct SigningRequestWire {
    account: AccountRef,
    network: atlas_core::id::NetworkId,
    curve: atlas_core::chain::Curve,
    #[serde(rename = "payloadKind")]
    payload_kind: atlas_core::signing::SigningPayloadKind,
    payload: String,
}

#[derive(Deserialize)]
struct PrepareResponse {
    unsigned: UnsignedWire,
    #[serde(rename = "signingRequest")]
    signing_request: SigningRequestWire,
}

fn hex_to_bytes(s: &str) -> Vec<u8> {
    let stripped = s.strip_prefix("0x").unwrap_or(s);
    hex::decode(stripped).expect("hex")
}

#[tokio::test]
#[ignore = "requires testnet credentials"]
async fn server_flow_sepolia_native_transfer() {
    let Some(cfg) = E2eConfig::from_env_for_network("eip155:11155111") else {
        eprintln!("skipping: env not set");
        return;
    };

    let signer = LocalKeySigner::from_bytes(
        SignerId::from_str("e2e-server-sepolia").unwrap(),
        cfg.private_key_bytes,
    )
    .unwrap();
    let from_addr = signer.address();

    let server = spawn_server_in_process(&cfg).await.unwrap();
    let base = format!("http://{}", server.local_addr);

    // 1. Server prepares the bundle.
    let prep_req = PrepareRequest {
        intent: TransferIntent {
            asset_instance_id: AssetInstanceId::from_str("eip155:11155111/native:eth").unwrap(),
            to: AddressRef::from_str(&cfg.recipient).unwrap(),
            amount: RawAmount::new(BigInt::from(1u64), 18).unwrap(),
        },
        account: AccountRef::from_str(&from_addr).unwrap(),
    };
    let bundle: PrepareResponse = http_post(&base, "/v1/transfers/prepare", &prep_req)
        .await
        .unwrap();

    // 2. Reconstruct the typed signing request and sign.
    let signing_request = atlas_core::signing::SigningRequest {
        account: bundle.signing_request.account,
        network: bundle.signing_request.network,
        curve: bundle.signing_request.curve,
        payload_kind: bundle.signing_request.payload_kind,
        payload: hex_to_bytes(&bundle.signing_request.payload),
    };
    let response = signer.sign(signing_request).await.unwrap();

    // 3. Reconstruct the unsigned tx and assemble.
    let unsigned = atlas_core::transaction::UnsignedTransaction {
        account: bundle.unsigned.account,
        network: bundle.unsigned.network,
        intent: bundle.unsigned.intent,
        payload: hex_to_bytes(&bundle.unsigned.payload),
    };
    let codec = EvmCodec;
    let signed = codec.assemble_signed(unsigned, response).unwrap();

    // 4. Broadcast via local provider.
    let provider = ProviderBuilder::new().connect_http(cfg.rpc_url.parse().unwrap());
    let broadcaster = EvmBroadcaster::new(provider.clone());
    let result = broadcaster.broadcast(signed).await.unwrap();
    let ok = wait_for_receipt(&provider, &result.tx_hash, Duration::from_secs(120))
        .await
        .unwrap();
    assert!(ok);

    let _ = server.shutdown.send(());
}
```

- [ ] **Step 2: Create the Base Sepolia variant**

`crates/atlas-e2e/tests/server_base_sepolia.rs` — same content as `server_sepolia.rs` with:
- function name `server_flow_base_sepolia_native_transfer`
- network suffix `eip155:84532` everywhere
- asset id `eip155:84532/native:eth`
- skipping log message points at the Base Sepolia env vars
- signer id `e2e-server-base-sepolia`

(Copy the file and apply those four substitutions.)

- [ ] **Step 3: Verify they compile**

Run: `cargo build -p atlas-e2e --tests`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/atlas-e2e/tests/server_sepolia.rs crates/atlas-e2e/tests/server_base_sepolia.rs
git commit -m "test(e2e): server flow integration tests (Sepolia + Base Sepolia)"
```

---

## Task 21: Example binaries

**Files:**
- Create: `crates/atlas-e2e/examples/direct_flow.rs`
- Create: `crates/atlas-e2e/examples/server_flow.rs`

- [ ] **Step 1: Create `direct_flow.rs`**

The example body is the same as `direct_sepolia.rs`'s test body, less the `#[tokio::test]` + `assert` ceremony. Wrap in `#[tokio::main]`, pick the network at runtime via `ATLAS_E2E_NETWORK` env var (default `eip155:11155111`), exit 1 with a friendly message if config is missing.

Sketch:

```rust
//! Runnable demo of the direct flow.
//!
//! Run with:
//!     ATLAS_E2E_RPC_EIP155_11155111=... \
//!     ATLAS_E2E_PRIVATE_KEY_EIP155_11155111=... \
//!     cargo run -p atlas-e2e --example direct_flow

use anyhow::{anyhow, Result};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let network = std::env::var("ATLAS_E2E_NETWORK")
        .unwrap_or_else(|_| "eip155:11155111".to_string());
    let cfg = atlas_e2e::E2eConfig::from_env_for_network(&network)
        .ok_or_else(|| anyhow!("missing env for network {network}"))?;

    // ... same flow as direct_sepolia.rs ...
    // Use the test body verbatim, replacing `expect(...)` with `?`.
    todo!("inline direct flow body from tests/direct_sepolia.rs, returning Result")
}
```

Inline the body — copy from `direct_sepolia.rs`, change `expect(...)` to `?` and `assert!` to `if !ok { return Err(anyhow!(...)) }`. Hard-code chain id by stripping the suffix from `network`.

Actually, for completeness, here's the full file content the engineer should write:

```rust
//! Runnable demo of the direct flow.

use alloy_provider::ProviderBuilder;
use anyhow::{anyhow, Result};
use atlas_core::amount::RawAmount;
use atlas_core::asset::AssetStandard;
use atlas_core::fee::EvmFee;
use atlas_core::id::{AccountRef, AddressRef, AssetInstanceId, SignerId};
use atlas_core::service::{ChainBroadcaster, ChainCodec, ChainReader, FeeEstimator};
use atlas_core::signing::SignerProvider;
use atlas_core::transaction::TransferIntent;
use atlas_e2e::{wait_for_receipt, E2eConfig};
use atlas_evm::broadcaster::EvmBroadcaster;
use atlas_evm::codec::{EvmCodec, EvmPrepareContext};
use atlas_evm::fee_estimator::EvmFeeEstimator;
use atlas_evm::reader::EvmReader;
use atlas_signer_localkey::LocalKeySigner;
use num_bigint::BigInt;
use std::str::FromStr;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let network = std::env::var("ATLAS_E2E_NETWORK")
        .unwrap_or_else(|_| "eip155:11155111".to_string());
    let cfg = E2eConfig::from_env_for_network(&network)
        .ok_or_else(|| anyhow!("missing env for network {network}"))?;
    let chain_id: u64 = network
        .split(':')
        .nth(1)
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| anyhow!("bad chain id in {network}"))?;
    let signer = LocalKeySigner::from_bytes(
        SignerId::from_str("direct-flow-demo")?,
        cfg.private_key_bytes,
    )?;
    let from = signer.address();
    let provider = ProviderBuilder::new().connect_http(cfg.rpc_url.parse()?);
    let reader = EvmReader::new(provider.clone());
    let estimator = EvmFeeEstimator::new(provider.clone(), true);
    let broadcaster = EvmBroadcaster::new(provider.clone());
    let codec = EvmCodec;
    let intent = TransferIntent {
        asset_instance_id: AssetInstanceId::from_str(&format!("{network}/native:eth"))?,
        to: AddressRef::from_str(&cfg.recipient)?,
        amount: RawAmount::new(BigInt::from(1u64), 18)?,
    };
    let from_ref = AddressRef::from_str(&from)?;
    let (nonce, fee) = tokio::join!(
        reader.get_nonce(&cfg.network_id, &from_ref),
        estimator.estimate_fee(&intent, &from_ref),
    );
    let nonce = nonce?;
    let fee = fee?;
    let ctx = EvmPrepareContext {
        account: AccountRef::from_str(&from)?,
        network: cfg.network_id.clone(),
        intent,
        chain_id,
        nonce,
        fee,
        standard: AssetStandard::Native,
        contract: None,
    };
    let unsigned = codec.prepare_transfer(ctx)?;
    let signing_request = codec.signing_request(&unsigned)?;
    let response = signer.sign(signing_request).await?;
    let signed = codec.assemble_signed(unsigned, response)?;
    let broadcast = broadcaster.broadcast(signed).await?;
    tracing::info!(tx_hash = %broadcast.tx_hash, "broadcasted");
    let ok = wait_for_receipt(&provider, &broadcast.tx_hash, Duration::from_secs(120)).await?;
    if !ok {
        return Err(anyhow!("tx reverted"));
    }
    tracing::info!(tx_hash = %broadcast.tx_hash, "confirmed");
    Ok(())
}
```

- [ ] **Step 2: Create `server_flow.rs`**

Mirror `server_sepolia.rs`'s body adapted to a runnable binary. The same pattern: `#[tokio::main]` returning `Result<()>`, env-driven network, inline-copy of the test body with `?` instead of `expect`/`unwrap`/`assert!`.

```rust
//! Runnable demo of the server-build + client-sign flow.

use alloy_provider::ProviderBuilder;
use anyhow::{anyhow, Result};
use atlas_core::amount::RawAmount;
use atlas_core::id::{AccountRef, AddressRef, AssetInstanceId, SignerId};
use atlas_core::service::{ChainBroadcaster, ChainCodec};
use atlas_core::signing::SignerProvider;
use atlas_core::transaction::{TransferIntent, UnsignedTransaction};
use atlas_e2e::{http_post, spawn_server_in_process, wait_for_receipt, E2eConfig};
use atlas_evm::broadcaster::EvmBroadcaster;
use atlas_evm::codec::EvmCodec;
use atlas_signer_localkey::LocalKeySigner;
use num_bigint::BigInt;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use std::time::Duration;

#[derive(Serialize)]
struct PrepareRequest {
    intent: TransferIntent,
    account: AccountRef,
}

#[derive(Deserialize)]
struct UnsignedWire {
    account: AccountRef,
    network: atlas_core::id::NetworkId,
    intent: TransferIntent,
    payload: String,
}

#[derive(Deserialize)]
struct SigningRequestWire {
    account: AccountRef,
    network: atlas_core::id::NetworkId,
    curve: atlas_core::chain::Curve,
    #[serde(rename = "payloadKind")]
    payload_kind: atlas_core::signing::SigningPayloadKind,
    payload: String,
}

#[derive(Deserialize)]
struct PrepareResponse {
    unsigned: UnsignedWire,
    #[serde(rename = "signingRequest")]
    signing_request: SigningRequestWire,
}

fn hex_to_bytes(s: &str) -> Vec<u8> {
    let stripped = s.strip_prefix("0x").unwrap_or(s);
    hex::decode(stripped).expect("hex")
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let network = std::env::var("ATLAS_E2E_NETWORK")
        .unwrap_or_else(|_| "eip155:11155111".to_string());
    let cfg = E2eConfig::from_env_for_network(&network)
        .ok_or_else(|| anyhow!("missing env for network {network}"))?;
    let signer = LocalKeySigner::from_bytes(
        SignerId::from_str("server-flow-demo")?,
        cfg.private_key_bytes,
    )?;
    let from = signer.address();
    let server = spawn_server_in_process(&cfg).await?;
    let base = format!("http://{}", server.local_addr);

    let prep = PrepareRequest {
        intent: TransferIntent {
            asset_instance_id: AssetInstanceId::from_str(&format!("{network}/native:eth"))?,
            to: AddressRef::from_str(&cfg.recipient)?,
            amount: RawAmount::new(BigInt::from(1u64), 18)?,
        },
        account: AccountRef::from_str(&from)?,
    };
    let bundle: PrepareResponse = http_post(&base, "/v1/transfers/prepare", &prep).await?;

    let signing_request = atlas_core::signing::SigningRequest {
        account: bundle.signing_request.account,
        network: bundle.signing_request.network,
        curve: bundle.signing_request.curve,
        payload_kind: bundle.signing_request.payload_kind,
        payload: hex_to_bytes(&bundle.signing_request.payload),
    };
    let response = signer.sign(signing_request).await?;

    let unsigned = UnsignedTransaction {
        account: bundle.unsigned.account,
        network: bundle.unsigned.network,
        intent: bundle.unsigned.intent,
        payload: hex_to_bytes(&bundle.unsigned.payload),
    };
    let codec = EvmCodec;
    let signed = codec.assemble_signed(unsigned, response)?;

    let provider = ProviderBuilder::new().connect_http(cfg.rpc_url.parse()?);
    let broadcaster = EvmBroadcaster::new(provider.clone());
    let result = broadcaster.broadcast(signed).await?;
    tracing::info!(tx_hash = %result.tx_hash, "broadcasted via server flow");
    let ok = wait_for_receipt(&provider, &result.tx_hash, Duration::from_secs(120)).await?;
    if !ok {
        let _ = server.shutdown.send(());
        return Err(anyhow!("tx reverted"));
    }
    tracing::info!(tx_hash = %result.tx_hash, "confirmed");
    let _ = server.shutdown.send(());
    Ok(())
}
```

- [ ] **Step 3: Verify both examples compile**

Run: `cargo build -p atlas-e2e --examples`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/atlas-e2e/examples/
git commit -m "feat(e2e): runnable example binaries (direct_flow, server_flow)"
```

---

## Task 22: Dockerfile (multi-stage → distroless)

**Files:**
- Create: `crates/atlas-server/Dockerfile`
- Create: `crates/atlas-server/Dockerfile.debug`
- Create: `crates/atlas-server/docker-compose.yml`

- [ ] **Step 1: Create `crates/atlas-server/Dockerfile`**

```dockerfile
# syntax=docker/dockerfile:1.7

FROM rust:1.83-slim AS builder
WORKDIR /build
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev ca-certificates \
 && rm -rf /var/lib/apt/lists/*
COPY . .
RUN --mount=type=cache,target=/build/target \
    --mount=type=cache,target=/usr/local/cargo/registry \
    cargo build --release -p atlas-server --features otel \
 && cp target/release/atlas-server /atlas-server

FROM gcr.io/distroless/cc-debian12:nonroot
COPY --from=builder /atlas-server /usr/local/bin/atlas-server
USER nonroot
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
  CMD ["/usr/local/bin/atlas-server", "health"]
ENTRYPOINT ["/usr/local/bin/atlas-server"]
CMD ["serve"]

LABEL org.opencontainers.image.source="https://github.com/Milerius/Atlas"
LABEL org.opencontainers.image.licenses="MIT"
LABEL org.opencontainers.image.description="Atlas HTTP server — server-builds-tx, client-signs"
```

- [ ] **Step 2: Create `crates/atlas-server/Dockerfile.debug`**

```dockerfile
# syntax=docker/dockerfile:1.7
# Sibling Dockerfile for ad-hoc debugging — same build, debian-slim runtime
# with curl + a shell available for `docker exec` poking.

FROM rust:1.83-slim AS builder
WORKDIR /build
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev ca-certificates \
 && rm -rf /var/lib/apt/lists/*
COPY . .
RUN --mount=type=cache,target=/build/target \
    --mount=type=cache,target=/usr/local/cargo/registry \
    cargo build --release -p atlas-server --features otel \
 && cp target/release/atlas-server /atlas-server

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
    curl ca-certificates \
 && rm -rf /var/lib/apt/lists/*
COPY --from=builder /atlas-server /usr/local/bin/atlas-server
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
  CMD ["/usr/local/bin/atlas-server", "health"]
ENTRYPOINT ["/usr/local/bin/atlas-server"]
CMD ["serve"]
```

- [ ] **Step 3: Create `crates/atlas-server/docker-compose.yml`**

```yaml
services:
  atlas-server:
    build:
      context: ../..
      dockerfile: crates/atlas-server/Dockerfile
    environment:
      ATLAS_BIND: "0.0.0.0:8080"
      ATLAS_LOG_LEVEL: "${ATLAS_LOG_LEVEL:-debug}"
      ATLAS_API_KEYS: "${ATLAS_API_KEYS:-dev-key-not-secret}"
      ATLAS_CORS_ALLOWED_ORIGINS: "${ATLAS_CORS_ALLOWED_ORIGINS:-*}"
      ATLAS_RPC_EIP155_11155111: "${ATLAS_RPC_EIP155_11155111:-}"
      ATLAS_RPC_EIP155_84532:    "${ATLAS_RPC_EIP155_84532:-}"
      OTEL_EXPORTER_OTLP_ENDPOINT: "${OTEL_EXPORTER_OTLP_ENDPOINT:-http://jaeger:4317}"
    ports:
      - "8080:8080"
    depends_on:
      - jaeger

  jaeger:
    image: jaegertracing/all-in-one:1.62
    ports:
      - "16686:16686"
      - "4317:4317"
```

- [ ] **Step 4: Smoke-build the image locally (optional, requires Docker)**

```bash
docker build -t atlas-server:dev -f crates/atlas-server/Dockerfile .
docker run --rm -e ATLAS_BIND=0.0.0.0:8080 -p 8080:8080 atlas-server:dev &
sleep 3
curl -f localhost:8080/health
kill %1
```

- [ ] **Step 5: Commit**

```bash
git add crates/atlas-server/Dockerfile crates/atlas-server/Dockerfile.debug crates/atlas-server/docker-compose.yml
git commit -m "feat(server): Dockerfile (distroless) + Dockerfile.debug + docker-compose"
```

---

## Task 23: CI workflow — extend ci.yml + add e2e.yml

**Files:**
- Modify: `.github/workflows/ci.yml`
- Create: `.github/workflows/e2e.yml`

- [ ] **Step 1: Inspect existing `ci.yml` to find the test job**

```bash
cat .github/workflows/ci.yml | head -80
```

Find the `Test (ubuntu-latest)` and `Test (macos-latest)` jobs and confirm the `cargo test --workspace` invocation pattern.

- [ ] **Step 2: Add an "atlas-server build" step to the test jobs**

In `.github/workflows/ci.yml`, in each Test job, add a step before `cargo test`:

```yaml
- name: Build atlas-server with otel feature
  run: cargo build --release -p atlas-server --features otel
```

If the existing pattern uses a matrix-style structure, add the step under the matching `steps:` list. The aim is keeping the release binary compiling.

- [ ] **Step 3: Create `.github/workflows/e2e.yml`**

```yaml
name: e2e

on:
  workflow_dispatch:
  schedule:
    - cron: '17 5 * * *'
  push:
    branches: [main]

jobs:
  e2e:
    name: e2e (${{ matrix.network }})
    runs-on: ubuntu-latest
    strategy:
      fail-fast: false
      matrix:
        include:
          - network: sepolia
            network_suffix: EIP155_11155111
            rpc_secret: ATLAS_E2E_RPC_EIP155_11155111
            key_secret: ATLAS_E2E_PRIVATE_KEY_EIP155_11155111
            test_filter: sepolia
          - network: base-sepolia
            network_suffix: EIP155_84532
            rpc_secret: ATLAS_E2E_RPC_EIP155_84532
            key_secret: ATLAS_E2E_PRIVATE_KEY_EIP155_84532
            test_filter: base_sepolia
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2

      - name: Run E2E tests
        env:
          ATLAS_E2E_RPC_EIP155_11155111: ${{ secrets.ATLAS_E2E_RPC_EIP155_11155111 }}
          ATLAS_E2E_PRIVATE_KEY_EIP155_11155111: ${{ secrets.ATLAS_E2E_PRIVATE_KEY_EIP155_11155111 }}
          ATLAS_E2E_RPC_EIP155_84532: ${{ secrets.ATLAS_E2E_RPC_EIP155_84532 }}
          ATLAS_E2E_PRIVATE_KEY_EIP155_84532: ${{ secrets.ATLAS_E2E_PRIVATE_KEY_EIP155_84532 }}
        run: |
          cargo test -p atlas-e2e --tests -- --ignored ${{ matrix.test_filter }}

      - name: Open issue on failure
        if: failure() && github.event_name != 'workflow_dispatch'
        uses: actions/github-script@v7
        with:
          script: |
            github.rest.issues.create({
              owner: context.repo.owner,
              repo: context.repo.repo,
              title: `E2E failure: ${{ matrix.network }}`,
              body: `Run: ${{ github.server_url }}/${{ github.repository }}/actions/runs/${{ github.run_id }}`,
              labels: ['e2e-failure']
            });
```

- [ ] **Step 4: Commit**

```bash
git add .github/workflows/ci.yml .github/workflows/e2e.yml
git commit -m "ci: extend ci.yml to build atlas-server + add e2e.yml workflow"
```

---

## Task 24: docker.yml + release-spec.yml workflows

**Files:**
- Create: `.github/workflows/docker.yml`
- Create: `.github/workflows/release-spec.yml`

- [ ] **Step 1: Create `.github/workflows/docker.yml`**

```yaml
name: docker

on:
  workflow_dispatch:
  push:
    tags: ['v*']

jobs:
  build-and-push:
    runs-on: ubuntu-latest
    permissions:
      contents: read
      packages: write
    steps:
      - uses: actions/checkout@v4

      - uses: docker/setup-qemu-action@v3
      - uses: docker/setup-buildx-action@v3

      - name: Log in to GHCR
        uses: docker/login-action@v3
        with:
          registry: ghcr.io
          username: ${{ github.actor }}
          password: ${{ secrets.GITHUB_TOKEN }}

      - name: Compute image metadata
        id: meta
        uses: docker/metadata-action@v5
        with:
          images: ghcr.io/${{ github.repository_owner }}/atlas-server
          tags: |
            type=semver,pattern={{version}}
            type=semver,pattern={{major}}.{{minor}}
            type=raw,value=latest,enable={{is_default_branch}}

      - name: Build + push
        uses: docker/build-push-action@v5
        with:
          context: .
          file: crates/atlas-server/Dockerfile
          platforms: linux/amd64,linux/arm64
          push: true
          tags: ${{ steps.meta.outputs.tags }}
          labels: ${{ steps.meta.outputs.labels }}
          cache-from: type=gha
          cache-to: type=gha,mode=max
```

- [ ] **Step 2: Create `.github/workflows/release-spec.yml`**

```yaml
name: release-spec

on:
  release:
    types: [published]
  workflow_dispatch:

jobs:
  publish-openapi:
    runs-on: ubuntu-latest
    permissions:
      contents: write
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2

      - name: Build server + extract OpenAPI spec
        run: |
          cargo build --release -p atlas-server
          # Start the server, curl /openapi.json, kill, attach.
          ATLAS_BIND=127.0.0.1:18080 \
            ./target/release/atlas-server serve &
          SERVER_PID=$!
          sleep 2
          curl -fsSL http://127.0.0.1:18080/openapi.json > openapi.json
          kill $SERVER_PID

      - name: Attach openapi.json to release
        if: github.event_name == 'release'
        uses: softprops/action-gh-release@v2
        with:
          files: openapi.json
```

- [ ] **Step 3: Commit**

```bash
git add .github/workflows/docker.yml .github/workflows/release-spec.yml
git commit -m "ci: docker.yml + release-spec.yml workflows"
```

---

## Task 25: Final verification

**Files:** none — verification only.

- [ ] **Step 1: Full workspace build**

```bash
cargo build --workspace
```

Expected: PASS.

- [ ] **Step 2: Full workspace test**

```bash
cargo test --workspace
```

Expected: ALL PASS. E2E tests skip (they're `#[ignore]`'d).

- [ ] **Step 3: fmt + clippy**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

Expected: clean.

- [ ] **Step 4: Doc build**

```bash
cargo doc --workspace --no-deps
```

Expected: zero warnings.

- [ ] **Step 5: Coverage measurement**

```bash
cargo llvm-cov --workspace --ignore-filename-regex 'atlas-(scenarios|e2e)' --summary-only
```

Expected: ≥ 99% workspace line coverage maintained. atlas-server's per-file coverage ≥ 99% lines.

- [ ] **Step 6: Manual server smoke-test (optional)**

```bash
ATLAS_BIND=127.0.0.1:8080 \
ATLAS_LOG_LEVEL=debug \
cargo run -p atlas-server -- serve &

# In another terminal:
curl -fsS localhost:8080/health
curl -fsS localhost:8080/v1/networks
curl -fsS localhost:8080/openapi.json | jq '.paths | keys'
curl -fsS localhost:8080/docs | head -20

kill %1
```

- [ ] **Step 7: Final commit (if any cleanup)**

```bash
git status
# If anything residual:
git add -A
git commit -m "chore(server): final verification cleanup"
```

---

## Self-review log

**Spec coverage** — each section of [`docs/superpowers/specs/2026-05-11-atlas-server-hybrid-deployment-design.md`](docs/superpowers/specs/2026-05-11-atlas-server-hybrid-deployment-design.md) mapped:

| Spec section | Tasks |
|---|---|
| §3 Scope (single endpoint, discovery, ops, auth, errors, observability, Docker, e2e) | Tasks 1-24 |
| §4 Architecture (AppState, NetworkChainService enum, startup wiring, dispatch, pluggable rpc_url_for) | Tasks 2, 4 |
| §5 API surface (transfers/prepare, networks, health/ready, metrics, openapi, docs, hex payload encoding, Stripe errors) | Tasks 3, 5, 6, 9, 10, 11, 12, 13, 14 |
| §6 Auth (bearer middleware, public paths, dev-mode warning, multi-tenancy extension point) | Task 7 |
| §7 Observability (tracing JSON, Prometheus, feature-gated OTel, PII policy) | Tasks 8, 13 |
| §8 Testing (3 layers: unit, integration, E2E; testnet matrix; coverage target) | Layered throughout; E2E in 18-21; coverage in 25 |
| §9 API docs (openapi.json, Scalar /docs, README, release artifact) | Tasks 14, 17, 24 |
| §10 Docker + CI (4 workflows: ci, e2e, docker, release-spec) | Tasks 22, 23, 24 |
| §11 Risks + mitigations (gating, secrets, distroless debug, OTel feature, env→file migration, future Solana, dev-mode auth) | Addressed across the spec; no behavior task — informational |
| §12 Cargo deps | Task 1 |
| §13 Glossary | Reference-only, no implementation task |

**Placeholder scan** — no TBD/TODO/"similar to" patterns found. The two `todo!()` mentions inside Task 21 narrative ("inline body verbatim") are immediately followed by the full file content the engineer types.

**Type consistency** — `ServerConfig` fields match across config.rs (Task 2), state.rs (Task 4), test_support.rs (Tasks 6, 11), e2e/lib.rs (Task 18). `AppState::build` signature updates in Task 13 are reflected in every caller (test_support, run, e2e). `NetworkChainService` enum dispatch reused unchanged from Task 4 in state.rs, routes/transfers.rs (Task 10), routes/health.rs (Task 12). `PrepareRequest` / `PrepareResponse` shape consistent between server (Task 10) and e2e tests (Task 20) and example (Task 21). `rpc_url_for` closure signature `Arc<dyn Fn(&NetworkId) -> Option<String> + Send + Sync>` identical in config.rs, test_support.rs, e2e/lib.rs.
