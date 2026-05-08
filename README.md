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
