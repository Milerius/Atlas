Feature: CAIP-validated typed ids and chain-aware address validation

  The typed-id boundary should catch malformed input before it reaches
  the registry or a chain service. Specifically: NetworkId is CAIP-2-
  shaped, AssetInstanceId is CAIP-19-shaped, and AddressRef can be
  validated against an AddressFormat (EIP-55 hex for EVM, base58 +
  length for Solana). This feature pins the contracts in prose.

  Scenario: AssetInstanceId exposes its CAIP-19 decomposition
    Given a registry loaded from the valid fixtures
    Then the asset instance "eip155:8453/erc20:0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913" decomposes to network "eip155:8453", asset namespace "erc20", asset reference "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913"

  Scenario: NetworkId construction rejects non-CAIP-2 input
    When I try to construct a NetworkId from "eip-155:1"
    Then it surfaces a CAIP validation error

  Scenario: An EIP-55-checksummed EVM address validates against the EVM format
    When I validate the address "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913" against the EVM format
    Then validation succeeds

  Scenario: A Solana base58 pubkey is rejected by the EVM format
    When I validate the address "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v" against the EVM format
    Then validation fails with an InvalidFormat error

  Scenario: A 32-byte base58 pubkey validates against the Solana format
    When I validate the address "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v" against the Solana format
    Then validation succeeds
