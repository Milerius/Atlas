Feature: Asset resolution through atlas-core

  Atlas-core's job at this layer is to take user-facing intent
  ("send USDC", "send ETH") and return the concrete on-chain
  asset instance that signs and broadcasts. Group → Instance
  resolution is the first half of that flow.

  Scenario: Resolve the USDC group to its concrete on-chain instances
    Given a registry loaded from the valid fixtures
    When I resolve asset instances for group "usdc"
    Then I get 2 instances
    And one of the instances has id "eip155:1/erc20:0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48"
    And one of the instances has id "eip155:8453/erc20:0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913"

  Scenario: Resolve the ETH group to its native instances
    Given a registry loaded from the valid fixtures
    When I resolve asset instances for group "eth"
    Then I get 2 instances
    And one of the instances has id "eip155:1/native:eth"
    And one of the instances has id "eip155:8453/native:eth"

  Scenario: Look up a concrete asset instance by exact id
    Given a registry loaded from the valid fixtures
    When I look up asset instance "eip155:8453/erc20:0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913"
    Then the instance network is "eip155:8453"

  Scenario: The valid fixtures register Ethereum and Base under the EVM chain
    Given a registry loaded from the valid fixtures
    Then the registry has 2 networks under the EVM chain
