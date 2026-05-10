Feature: Wallet-side signer wiring through the chain registry

  A wallet integration shouldn't hardcode chain-specific HD paths
  at every call site. Atlas's chain registry carries the canonical
  BIP-32 / SLIP-44 path on each `Chain` entry, and signers consume
  it from there. This feature pins the contract: the registry
  exposes the path, and a signer derived from it lands on the
  expected address.

  Scenario: The EVM chain entry exposes its canonical BIP-32 derivation path
    Given a registry loaded from the valid fixtures
    Then the EVM chain default derivation path is "m/44'/60'/0'/0/0"

  Scenario: A LocalKeySigner derived via the registry path lands on the canonical address
    Given a registry loaded from the valid fixtures
    When I derive a LocalKeySigner from the BIP-39 test mnemonic using the EVM chain's default path
    Then the signer address is "0x9858EfFD232B4033E47d90003D41EC34EcaEda94"
