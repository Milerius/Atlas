# Atlas Server — Hybrid Deployment Design

**Status:** design  
**Date:** 2026-05-11  
**Author:** Atlas core team (with Claude Opus 4.7)  

## 1. Goal

Ship an HTTP server, packaged for Docker, that exposes Atlas's "server-builds-tx, client-signs-and-broadcasts" deployment shape through a real API. Prove the SDK works end-to-end against two public testnets (Sepolia and Base Sepolia) via both the direct client flow (no server) and the server-build flow. Establish the foundation for future API modules (balance, portfolio, swap aggregator) plugging into the same server.

## 2. Context

The SDK side of this is already done:

- The 5-trait split in `atlas_core::service` separates the pure codec from the RPC-bound reader / fee-estimator / broadcaster.
- `EvmChainService::prepare_unsigned_bundle` accepts a `TransferIntent + AccountRef`, fetches nonce and fee, runs the codec, and returns an `UnsignedBundle` containing the unsigned RLP plus the pre-computed signing request. No signer required.
- `EvmChainService::assemble_and_broadcast` takes an unsigned tx plus a `SigningResponse` and broadcasts. No reader or fee estimator required.
- The wire types (`TransferIntent`, `UnsignedBundle`, `UnsignedTransaction`, `SigningRequest`, `SigningResponse`, `BroadcastResult`) derive `Serialize + Deserialize`.
- `crates/atlas-evm/tests/split_host_flow.rs` proves the architecture works locally with two mocked alloy providers.

What this design adds: a real HTTP service over `prepare_unsigned_bundle`, the operational scaffolding around it (auth, config, observability, Docker), and an E2E proof against real testnets.

## 3. Scope

### In scope (v1)

- One server crate, `atlas-server`, exposing both a binary entry point and a library `run(config)` for embedded tests.
- One work-doing endpoint: `POST /v1/transfers/prepare`.
- Discovery and operational endpoints: `GET /v1/networks`, `GET /health`, `GET /ready`, `GET /metrics`, `GET /openapi.json`, `GET /docs`.
- Bearer-token middleware on protected routes; public on the discovery and operational set.
- Stripe-style error bodies.
- Observability stack: structured `tracing` JSON logs, Prometheus metrics, OpenTelemetry traces (feature-gated).
- Multi-stage Dockerfile producing a distroless image with a self-probe healthcheck.
- One test crate, `atlas-e2e` (`publish = false`), containing integration tests and example binaries that demonstrate both flows (direct + server) against Sepolia and Base Sepolia.

### Explicitly out of scope (v1)

- Server-side broadcast. The eventual NaaS layer is what the client's broadcaster will talk to; we're not building NaaS here.
- Server-side signing of any kind.
- Multi-tenancy backed by a database. The middleware shape supports it; v1 backend is a `HashSet<String>` of bearer tokens from env.
- `ApproveIntent` and `ContractCallIntent`. Transfer only.
- Solana endpoints. atlas-solana isn't landed; the dispatcher is structured to accept Solana when it does.
- Rate limiting. That belongs in a reverse proxy.
- A user-facing CLI (`atlas-cli`). Possibly later if demand materialises.

## 4. Architecture

```text
                  ┌───────────────────────────────────────────────┐
                  │              atlas-server (crate)             │
                  │                                               │
                  │   bin: atlas-server  ◄──── main.rs            │
                  │   lib:               ◄──── pub fn run(cfg)    │
                  │                            -> JoinHandle      │
                  │                                               │
                  │   ┌─────────────────────────────────────────┐ │
                  │   │  axum::Router                            │ │
                  │   │   /v1/transfers/prepare   POST           │ │
                  │   │   /v1/networks            GET            │ │
                  │   │   /health, /ready         GET            │ │
                  │   │   /metrics, /openapi.json GET            │ │
                  │   │   /docs                   GET            │ │
                  │   │   middleware: bearer, cors, trace, otel  │ │
                  │   └─────────────────────────────────────────┘ │
                  │   ┌─────────────────────────────────────────┐ │
                  │   │  AppState                                │ │
                  │   │   registry: Arc<Registry>                │ │
                  │   │   chain_services: HashMap<NetworkId,     │ │
                  │   │                  NetworkChainService>    │ │
                  │   │   api_keys: Arc<HashSet<String>>         │ │
                  │   │   rpc_url_for: Arc<dyn Fn(&NetworkId)    │ │
                  │   │                       -> Option<String>> │ │
                  │   │   started_at: Instant                    │ │
                  │   │   version: &'static str                  │ │
                  │   └─────────────────────────────────────────┘ │
                  └────────────────────┬──────────────────────────┘
                                       │
              ┌────────────────────────┴─────────────────────────┐
              │                                                  │
   atlas-evm                                              atlas-core
   EvmChainService<RootProvider<Http>>                    Registry, wire types,
   alloy 2.x, RPC reads (nonce, fee)                      typed errors
```

```text
                          ┌────────────────────────────┐
                          │   atlas-e2e (test crate)   │
                          │                            │
                          │   tests/                   │
                          │     direct_sepolia.rs      │
                          │     direct_base_sepolia.rs │
                          │     server_sepolia.rs      │
                          │     server_base_sepolia.rs │
                          │   examples/                │
                          │     direct_flow.rs         │
                          │     server_flow.rs         │
                          │                            │
                          │   deps: atlas-evm,         │
                          │         atlas-server,      │
                          │         atlas-signer-localkey │
                          │                            │
                          │   gated on:                │
                          │     ATLAS_E2E_RPC_*        │
                          │     ATLAS_E2E_PRIVATE_KEY_*│
                          └────────────────────────────┘
```

### Service composition

```rust
// crates/atlas-server/src/state.rs

pub struct AppState {
    pub registry: Arc<Registry>,
    pub chain_services: Arc<HashMap<NetworkId, NetworkChainService>>,
    pub api_keys: Arc<HashSet<String>>,
    pub started_at: Instant,
    pub version: &'static str,
}

/// Enum dispatch over concrete chain services. Trait objects don't
/// compose with `ChainService`'s generic associated types cleanly,
/// so we use enum dispatch — one variant per chain family, plus an
/// Unconfigured variant for networks that are in the registry but
/// lack an RPC URL or live in a namespace we don't yet support.
pub enum NetworkChainService {
    Evm(EvmChainService<RootProvider<Http>>),
    Unconfigured { reason: UnconfiguredReason },
    // future: Solana(SolanaChainService<...>),
}

pub enum UnconfiguredReason {
    NoRpcUrl,
    UnknownNamespace(String),
}
```

### Startup wiring

This mirrors blockchain-kit's `signingModule` pattern adapted for Rust:

1. Parse `ServerConfig` from env vars via `from_env()`.
2. Load the bundled atlas-core official registry.
3. For each `Network` in the registry:
   - Resolve the RPC URL via `config.rpc_url_for(&network.id)` — the injected closure.
   - If `Some(url)`: build a `RootProvider<Http>`, wrap in `EvmChainService::new(provider, network.id.clone(), chain_id, eip1559)`, store as `NetworkChainService::Evm(svc)`.
   - If `None`: store as `NetworkChainService::Unconfigured { NoRpcUrl }`.
4. For namespaces atlas-server doesn't handle (e.g. `solana` until atlas-solana ships): `NetworkChainService::Unconfigured { UnknownNamespace(ns.into()) }`.

### Per-request dispatch

1. Deserialise the request body. CAIP-19 parse on `intent.asset_instance_id` happens at deserialisation time via the custom `Deserialize` impl in atlas-core — malformed IDs return 400 before any handler logic runs.
2. `intent.asset_instance_id.network_id()` → look up in `chain_services`.
3. Match the variant:
   - `Evm(svc)`: call `svc.prepare_unsigned_bundle(intent, account).await`, map result to HTTP.
   - `Unconfigured { NoRpcUrl }`: return 503 with `network_unconfigured`.
   - `Unconfigured { UnknownNamespace(_) }`: return 400 with `unsupported_asset`.
4. If the network isn't in the registry at all: return 400 with `unsupported_asset`.

### Pluggable RPC URL resolution

```rust
pub struct ServerConfig {
    pub bind: SocketAddr,
    pub registry: Registry,
    pub api_keys: HashSet<String>,
    pub rpc_url_for: Arc<dyn Fn(&NetworkId) -> Option<String> + Send + Sync>,
    pub cors_allowed_origins: CorsOrigins,
    pub log_level: tracing::Level,
    pub otel_endpoint: Option<String>,
}

impl ServerConfig {
    /// Default resolver: env var `ATLAS_RPC_<normalised_network_id>`.
    /// `eip155:11155111` → `ATLAS_RPC_EIP155_11155111`.
    pub fn from_env() -> Result<Self, ConfigError> { ... }
}
```

Library consumers (a hypothetical NaaS server crate, an embedded test harness, a vault-backed deployment) replace `rpc_url_for` with their own resolver without touching atlas-server's internals.

## 5. API surface

### POST /v1/transfers/prepare

```
Request body:
{
  "intent": {
    "assetInstanceId": "eip155:11155111/native:eth",
    "to":   "0x0000000000000000000000000000000000000003",
    "amount": { "value": "10000000000000000", "decimals": 18 }
  },
  "account": "0x9858EfFD232B4033E47d90003D41EC34EcaEda94"
}

200 OK
{
  "unsigned": {
    "account": "0x9858EfFD232B4033E47d90003D41EC34EcaEda94",
    "network": "eip155:11155111",
    "intent":  { ... },
    "payload": "0x02f86b837aa68a..."
  },
  "signingRequest": {
    "account":     "0x...",
    "network":     "eip155:11155111",
    "curve":       "secp256k1",
    "payloadKind": "transaction_digest",
    "payload":     "0xa1b2c3..."
  }
}
```

#### Payload encoding

`payload: Vec<u8>` in atlas-core serdes to a JSON array of integers by default. The server response wraps `unsigned.payload` and `signingRequest.payload` as **hex strings with `0x` prefix** — Web3 convention. The request side accepts both formats via a permissive deserialise wrapper. Atlas-core itself stays bytes-typed; hex wrapping happens only at the server's HTTP boundary.

### GET /v1/networks

```
200 OK
[
  { "id": "eip155:11155111", "name": "Sepolia",      "namespace": "eip155",
    "configured": true,  "features": {"eip1559": true} },
  { "id": "eip155:84532",    "name": "Base Sepolia", "namespace": "eip155",
    "configured": true,  "features": {"eip1559": true, "opStackL1Fee": true} },
  { "id": "eip155:1",        "name": "Ethereum",     "namespace": "eip155",
    "configured": false, "features": {"eip1559": true} }
]
```

`configured: false` means the registry knows the network but no RPC URL is wired. Useful for the "did I forget to set an env var" debugging story.

### GET /health, GET /ready

`/health` is a liveness probe — returns 200 + `{"status": "ok", "version": "...", "uptime_seconds": N}` as long as the process is alive.

`/ready` is a readiness probe — returns 200 only if all *configured* RPCs respond to `eth_blockNumber` within 2 seconds. 503 otherwise. Response body lists per-network status.

### GET /metrics

Prometheus text exposition format. Includes:

- `atlas_http_requests_total{route, method, status}`
- `atlas_http_request_duration_seconds{route, method}` (histogram)
- `atlas_chain_calls_total{network, operation, result}`
- `atlas_chain_call_duration_seconds{network, operation}` (histogram)
- `atlas_rpc_calls_total{network, method, result}`
- `atlas_rpc_call_duration_seconds{network, method}` (histogram)
- Default process metrics

### GET /openapi.json, GET /docs

`/openapi.json` serves the utoipa-generated OpenAPI 3.x spec. `/docs` serves an HTML page embedding [Scalar API Reference](https://github.com/scalar/scalar) pointing at `/openapi.json`. Scalar bundle is embedded via `include_bytes!` for offline deploys.

### Error model — Stripe-style

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

| HTTP | `type` | Example `code`s |
|---|---|---|
| 400 | `invalid_request_error` | `invalid_caip`, `invalid_address`, `negative_amount`, `decimals_mismatch`, `unsupported_asset`, `unsupported_standard`, `invalid_request_body` |
| 401 | `auth_error` | `missing_token`, `invalid_token` |
| 500 | `api_error` | `build_failed`, `internal_error` |
| 502 | `rpc_error` | `rpc_unavailable`, `rpc_node_error`, `rpc_malformed_response` |
| 503 | `api_error` | `network_unconfigured` |

`param` is the JSON-path of the offending field (`intent.assetInstanceId`, `intent.to`, `intent.amount.value`, `account`) where determinable. `message` is the inner typed error's `Display` — never includes RPC URLs, stack traces, or PII.

Implementation:

```rust
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error(transparent)] Chain(#[from] atlas_core::ChainError),
    #[error(transparent)] Caip(#[from] atlas_core::CaipError),
    #[error(transparent)] Address(#[from] atlas_core::AddressError),
    #[error("network {0} is not configured")]
    NetworkUnconfigured(NetworkId),
    #[error("missing or invalid bearer token")]
    Unauthorized { code: AuthCode },
    // ...
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
```

## 6. Auth

```rust
// crates/atlas-server/src/middleware/auth.rs

pub async fn require_bearer(
    State(state): State<AppState>,
    req: Request<Body>,
    next: Next,
) -> Result<Response, ApiError> {
    let path = req.uri().path();
    if PUBLIC_PATHS.iter().any(|p| path == *p || path.starts_with(p)) {
        return Ok(next.run(req).await);
    }
    if state.api_keys.is_empty() {
        // Dev mode: emit one-time warn at startup, allow all requests.
        // Production deployment SHOULD set keys; we don't fail-closed
        // because that breaks local-dev / docker-compose UX.
        return Ok(next.run(req).await);
    }
    let token = req
        .headers()
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or(ApiError::missing_token())?;
    if !state.api_keys.contains(token) {
        return Err(ApiError::invalid_token());
    }
    Ok(next.run(req).await)
}

const PUBLIC_PATHS: &[&str] = &[
    "/health", "/ready", "/metrics", "/openapi.json", "/docs", "/v1/networks",
];
```

`ATLAS_API_KEYS=key1,key2,key3` (comma-separated; rotation = add new, remove old).

**Extension point.** Multi-tenancy: the same middleware resolves `token → TenantContext` via a `TenantResolver` trait stored in AppState, attaches the resolved context to request extensions, downstream handlers extract via `Extension<TenantContext>`. The shape change is additive — v1 integration tests keep passing.

**CORS.** `tower-http::cors` middleware. `ATLAS_CORS_ALLOWED_ORIGINS=*` for dev, allowlist for prod (comma-separated origins).

## 7. Observability

```rust
// crates/atlas-server/src/telemetry.rs

pub fn init(cfg: &ServerConfig) -> Result<TelemetryGuard, anyhow::Error> {
    let fmt_layer = tracing_subscriber::fmt::layer()
        .json()                       // structured JSON to stdout
        .with_target(true)
        .with_thread_ids(true);
    let metrics_layer = tracing_metrics_layer();
    let otel_layer = cfg.otel_endpoint.as_ref().map(|endpoint| {
        let tracer = opentelemetry_otlp::new_pipeline()
            .tracing()
            .with_exporter(opentelemetry_otlp::new_exporter().tonic().with_endpoint(endpoint))
            .install_batch(opentelemetry::runtime::Tokio)?;
        tracing_opentelemetry::layer().with_tracer(tracer)
    });
    let filter = EnvFilter::from_default_env()
        .add_directive(format!("{}", cfg.log_level).parse()?);

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt_layer)
        .with(metrics_layer)
        .with(otel_layer)
        .init();

    let prom_handle = setup_prometheus_recorder()?;
    Ok(TelemetryGuard { prom_handle })
}
```

Spans on every public handler. Nested spans on every `ChainService` call and every alloy RPC call. Span attributes: `chain_id`, `network`, `asset_namespace`, `method`. **Never logged or traced: account addresses, recipient addresses, token amounts, RPC URLs.** Codified in the spec as a clippy-style review checklist; PRs are reviewed for accidental PII leaks.

OpenTelemetry export is feature-gated:

```toml
[features]
default = []
otel = ["tracing-opentelemetry", "opentelemetry-otlp", "opentelemetry"]
```

Production builds enable the feature; local dev / unit tests don't pay the dep cost.

## 8. Testing strategy

Three layers, mapped onto Atlas's existing patterns:

```text
┌────────────────────────────────────────────────────────────────┐
│  Layer 1: unit tests (crates/atlas-server/src/**)              │
│                                                                │
│   error.rs           ApiError → HTTP status + JSON body        │
│   middleware/auth    public path bypass, bearer matching       │
│   chain.rs           CAIP-2 namespace → NetworkChainService    │
│   state.rs           ServerConfig::from_env() permutations     │
│                                                                │
│   axum::test_helpers::TestServer + MockEvmChainService         │
│   No socket, no real HTTP, no tokio runtime alloc              │
└────────────────────────────────────────────────────────────────┘
                              ▼ runs on every PR
┌────────────────────────────────────────────────────────────────┐
│  Layer 2: integration tests (crates/atlas-server/tests/*.rs)   │
│                                                                │
│   server_prepare.rs   POST /v1/transfers/prepare happy path    │
│                       (mocked alloy Provider via Asserter)     │
│   server_errors.rs    400/401/502/503 paths                    │
│   server_networks.rs  GET /v1/networks discovery               │
│   server_health.rs    /health, /ready behaviour                │
│   server_openapi.rs   /openapi.json validates as OpenAPI 3.x   │
│   server_metrics.rs   /metrics serves Prometheus exposition    │
│                                                                │
│   Real axum + real reqwest + random port; mocked alloy         │
└────────────────────────────────────────────────────────────────┘
                              ▼ runs on every PR
┌────────────────────────────────────────────────────────────────┐
│  Layer 3: E2E tests (crates/atlas-e2e/tests/*.rs, gated)       │
│                                                                │
│   direct_sepolia.rs           SDK direct → real Sepolia        │
│   direct_base_sepolia.rs      SDK direct → real Base Sepolia   │
│   server_sepolia.rs           Server → sign → real Sepolia     │
│   server_base_sepolia.rs      Server → sign → real Base Sepolia│
│                                                                │
│   Real alloy Provider, real testnet, real LocalKeySigner;      │
│   gated on ATLAS_E2E_RPC_*, ATLAS_E2E_PRIVATE_KEY_* env vars;  │
│   tests skip cleanly when env not set.                         │
└────────────────────────────────────────────────────────────────┘
                              ▼ runs nightly + on-merge to main
```

TDD discipline for the implementation plan: each route handler ships as a unit-test-first commit, then integration test, then implementation. Standard red-green-refactor. Layer 1 + Layer 2 are mandatory; they're part of every PR's `cargo test --workspace` gate.

### E2E concrete flow (server, Sepolia)

```rust
#[tokio::test]
async fn server_flow_sepolia_native_transfer() {
    let Some(cfg) = E2eConfig::from_env_for_network("eip155:11155111") else {
        return; // graceful skip; logs explanation
    };
    let signer = LocalKeySigner::from_bytes(SignerId::new("e2e")?, cfg.private_key_bytes)?;
    let from   = signer.address();
    let server = atlas_e2e::spawn_server_in_process(&cfg).await?;

    // 1. Ask the server to prepare an unsigned bundle.
    let bundle: UnsignedBundle = http_post(&server.url, "/v1/transfers/prepare", &PrepareRequest {
        intent: TransferIntent {
            asset_instance_id: AssetInstanceId::from_str("eip155:11155111/native:eth")?,
            to:     AddressRef::from_str(&cfg.recipient)?,
            amount: RawAmount::new(BigInt::from(1u64), 18)?,
        },
        account: AccountRef::from_str(&from)?,
    }).await?;

    // 2. Client signs locally — server never sees the key.
    let response = signer.sign(bundle.signing_request).await?;

    // 3. Client broadcasts via its own RootProvider.
    let provider = ProviderBuilder::new().connect_http(cfg.rpc_url.parse()?);
    let broadcaster = EvmBroadcaster::new(provider.clone());
    let codec = EvmCodec;
    let signed = codec.assemble_signed(bundle.unsigned, response)?;
    let result = broadcaster.broadcast(signed).await?;

    // 4. Wait for receipt (poll up to 60s).
    let receipt = wait_for_receipt(&provider, &result.tx_hash, Duration::from_secs(60)).await?;
    assert_eq!(receipt.status, true, "tx must succeed");

    server.shutdown().await;
}
```

### Testnet account funding

One funded account per network, key stored as GitHub Actions encrypted secrets (`ATLAS_E2E_PRIVATE_KEY_SEPOLIA`, `ATLAS_E2E_PRIVATE_KEY_BASE_SEPOLIA`). Each E2E run transfers 1 wei to a known recipient — gas fees come from the account, principal stays roughly stable. Faucet top-up is documented in `crates/atlas-e2e/README.md`. Watchdog: if balance drops below a threshold, the workflow opens a GitHub issue rather than silently failing.

### Coverage targets

The existing bar: ≥ 99% workspace line coverage, all files ≥ 99% lines (per [codecov.yml](codecov.yml)). atlas-server inherits. **atlas-e2e is excluded from coverage** (same pattern as atlas-scenarios — `--ignore-filename-regex 'atlas-(scenarios|e2e)'`); it's E2E infrastructure, not production code.

## 9. API documentation

Three layers, all generated from one source of truth.

```text
                ┌──────────────────────────────────────────┐
                │   utoipa derive macros on route fns +    │
                │   ToSchema derives on wire types         │
                │                                          │
                │      ↓ generates at compile time         │
                │                                          │
                │   utoipa::OpenApi → openapi.json         │
                │   (machine-readable, OpenAPI 3.x)        │
                └──────────────────┬───────────────────────┘
                                   │
              ┌────────────────────┼────────────────────────┐
              ▼                    ▼                        ▼
   /openapi.json          /docs (Scalar UI)         openapi.json release
   (raw spec for          (interactive,             artifact (consumed by
    machine consumers)     browsable docs)           openapi-generator
                                                     for client SDKs)
```

| Surface | Path / location | Purpose |
|---|---|---|
| Raw OpenAPI 3.x spec | `GET /openapi.json` | Machine-readable spec. Source of truth. |
| Interactive API explorer | `GET /docs` | Scalar UI embed pointing at `/openapi.json`. Try-it-out forms, search, code samples (curl / JS / Python / Rust). |
| Operator README | `crates/atlas-server/README.md` | Self-host docs: env var reference, Docker compose example, curl examples per endpoint, error-code table, auth setup. |
| Released spec artifact | GitHub release attachment | `openapi.json` attached to every tagged release. Lets consumers code-gen client SDKs (`@atlas/sdk-typescript`, etc.) via `openapi-generator-cli`. |

utoipa annotations are mandatory on every public route and every request/response/error type — enforced by `#[warn(missing_docs)]` on the crate. `/openapi.json` and `/docs` are public routes (no bearer required) so operators and clients can introspect.

## 10. Docker + CI

### Dockerfile

```dockerfile
# syntax=docker/dockerfile:1.7
FROM rust:1.83-slim AS builder
WORKDIR /build
COPY . .
RUN --mount=type=cache,target=/build/target \
    --mount=type=cache,target=/usr/local/cargo/registry \
    cargo build --release -p atlas-server --features otel && \
    cp target/release/atlas-server /atlas-server

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
```

`atlas-server health` is a binary subcommand that HTTP-probes `localhost:$ATLAS_BIND/health` and exits 0/1. Distroless has no curl; this replaces it.

A sibling `Dockerfile.debug` with `FROM debian:bookworm-slim` runtime stays available for ad-hoc `docker exec` investigation.

### docker-compose.yml (local dev)

```yaml
services:
  atlas-server:
    build: { context: ., target: builder }
    environment:
      ATLAS_BIND: 0.0.0.0:8080
      ATLAS_RPC_EIP155_11155111: ${ATLAS_RPC_EIP155_11155111}
      ATLAS_RPC_EIP155_84532:    ${ATLAS_RPC_EIP155_84532}
      ATLAS_API_KEYS:            dev-key-not-secret
      ATLAS_LOG_LEVEL:           debug
      OTEL_EXPORTER_OTLP_ENDPOINT: http://jaeger:4317
    ports: ["8080:8080"]
    depends_on: [jaeger]

  jaeger:
    image: jaegertracing/all-in-one:1.62
    ports: ["16686:16686", "4317:4317"]
```

`.env.example` ships with placeholder testnet RPC URLs and the dev API key.

### GitHub Actions workflows

| Workflow | Trigger | What runs |
|---|---|---|
| `ci.yml` (existing, extended) | PR + push to main | fmt, clippy, unit, integration, BDD, coverage. Adds: `cargo build --release -p atlas-server` to keep the binary compiling. |
| `e2e.yml` (new) | Manual + nightly + on `main` merge | `cargo test -p atlas-e2e -- --ignored` with env from secrets. Per-network matrix (Sepolia, Base Sepolia). Failure opens an issue, doesn't gate merges. |
| `docker.yml` (new) | Push tag `v*` + manual | Multi-arch build (linux/amd64, linux/arm64) via `docker/build-push-action@v5`, publish to `ghcr.io/milerius/atlas-server:<tag>` and `:latest`. |
| `release-spec.yml` (new) | Release tagged | Attaches `openapi.json` build artifact to the GitHub release. |

## 11. Risks + mitigations

| Risk | Mitigation |
|---|---|
| Testnet flakiness gates merges | E2E workflow doesn't gate PRs; failure opens an issue. Layer 1 + Layer 2 tests are the merge gate. |
| Funded testnet key leaks | Encrypted GH secret, only injected into the e2e workflow context. Tests log only addresses, never keys. |
| Distroless image lacks debug tools | Production stays distroless. `Dockerfile.debug` sibling for ad-hoc investigation. |
| OpenTelemetry adds dep weight | Feature-gated. Default builds don't pay the cost. |
| Server outgrows env-var config (10+ networks) | `ServerConfig::from_env` becomes `ServerConfig::from_env_or_file` additively. Existing deploys keep working. |
| Future Solana endpoint needs a different request shape | Enum dispatch over `NetworkChainService` already accommodates it. The HTTP endpoint stays one shape; the dispatcher picks the right service per intent. |
| Bearer-token middleware doesn't fail-closed in dev mode | Documented explicitly: empty `ATLAS_API_KEYS` triggers a one-time warning at startup. Production deployments are expected to set keys; `docker.yml` workflow tests that the warning fires when keys are unset. |

## 12. Cargo dependencies

New workspace deps (added to `Cargo.toml`):

- `axum` — HTTP framework
- `tower`, `tower-http` — middleware (CORS, trace, request body limit)
- `utoipa`, `utoipa-axum` — OpenAPI 3.x derive
- `reqwest` — used by atlas-e2e + the `atlas-server health` subcommand
- `tracing-subscriber`, `tracing-opentelemetry` (feature-gated)
- `metrics`, `metrics-exporter-prometheus`, `metrics-tracing-context`
- `opentelemetry`, `opentelemetry-otlp` (feature-gated)
- `hex` — for the hex-encoding wrapper at the HTTP boundary
- `anyhow` — error wrapping inside the server crate
- `clap` — CLI subcommands (`serve`, `health`, `ready`)

## 13. Glossary

| Term | Meaning |
|---|---|
| **Direct flow** | Client uses `atlas-evm` + `atlas-signer-localkey` directly. No server in the picture. Broadcasts via the client's own `RootProvider`. |
| **Server-build flow** | Client POSTs `TransferIntent + AccountRef` to atlas-server, receives `UnsignedBundle`, signs locally, broadcasts via the client's own provider. |
| **Build-only** | Server's responsibility shape: it builds unsigned bundles, the client signs and broadcasts. Server has zero key material. |
| **`UnsignedBundle`** | Atlas-core wire type: `{ unsigned: UnsignedTransaction, signing_request: SigningRequest }`. The atomic payload the server returns. |
| **Pluggable RPC resolution** | The `rpc_url_for: Arc<dyn Fn(&NetworkId) -> Option<String>>` closure on `ServerConfig`. Lets library consumers swap env-var lookup for Vault, NaaS, anything. |
| **NaaS** | Node-as-a-Service. A future Atlas layer that load-balances RPC requests across multiple providers. Out of scope for v1. |
