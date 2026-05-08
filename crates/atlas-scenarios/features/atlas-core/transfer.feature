Feature: End-to-end mock transfer through the chain service boundary

  The chain service boundary takes a concrete AssetInstance, runs
  prepare → signing-request → assemble → broadcast, and returns
  a tx hash. With the mock service + mock signer wired up, a
  successful flow returns "0xmock". A network mismatch is rejected
  before signing.

  Scenario: Send ERC-20 USDC on Base end-to-end with the mock service
    Given a registry loaded from the valid fixtures
    And a mock EVM chain service
    And a mock signer named "mock-signer"
    When I prepare a transfer of 100000000 base units to "0x0000000000000000000000000000000000000001" using instance "eip155:8453/erc20:0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913" on network "eip155:8453"
    Then the broadcast tx hash is "0xmock"

  Scenario: Send native ETH on Ethereum end-to-end with the mock service
    Given a registry loaded from the valid fixtures
    And a mock EVM chain service
    And a mock signer named "mock-signer"
    When I prepare a transfer of 1000000000000000000 base units to "0x0000000000000000000000000000000000000001" using instance "eip155:1/native:eth" on network "eip155:1"
    Then the broadcast tx hash is "0xmock"

  Scenario: Transfer is rejected when the asset instance belongs to a different network
    Given a registry loaded from the valid fixtures
    And a mock EVM chain service
    And a mock signer named "mock-signer"
    When I prepare a transfer of 1 base units to "0x0000000000000000000000000000000000000001" using instance "eip155:8453/native:eth" on network "eip155:1"
    Then the transfer is rejected as unsupported asset instance
