# Atlas Product Accounts And Wallet Composition Notes

## Summary

This document captures the product-account brainstorming that sits above the
first blockchain core layer. It is intentionally separate from the initial core
implementation plan because `Chain`, `Network`, `AssetGroup`,
`AssetInstrument`, `AssetInstance`, registries, signing, and chain services
should remain the stable low-level base.

Product accounts are the app-facing composition layer. They let a wallet,
neobank, trading app, or portfolio app expose experiences such as Credit Card,
Investment Hub, Savings Earn, and future modules without forcing one custody,
signing, chain, or product style onto SDK consumers.

The key rule:

```text
ProductAccount composes assets, balances, positions, routes, policies, and
signing preferences.

ProductAccount does not replace Chain, Network, AssetInstrument, or
AssetInstance.
```

## Why Product Accounts Exist

Mobile wallet apps often do not present users with raw chains and contracts.
They present account-like products:

- Credit Card Account: card balance, top-up, freeze, spend limits, repayment.
- Investment Account: crypto, stocks, tokenized stocks, ETFs, funds, portfolio
  allocation, recurring buys.
- Savings Earn Account: vaults, yield positions, staking, lending, withdrawals.
- Future accounts: trading, business treasury, tax, rewards, payroll, remittance.

Atlas should make these products easy to build, but it should not hard-code
that trading must use account abstraction, savings must use MPC, or card must be
custodial. Those are app choices.

## Layering

The preferred layering is:

```text
Mobile App / Flutter UI
  -> ProductAccount layer
      -> Portfolio / Balance / Routing layer
          -> AssetGroup / AssetInstrument / AssetInstance registry
              -> ChainService
                  -> SignerProvider
                      -> MPC | Local Key | Privy | Account Abstraction | Future
```

Concrete flow:

```text
User taps "Deposit USDC" in Savings Earn
  -> SavingsEarnAccount creates a product action
  -> Resolver chooses exact AssetInstance, route, and network
  -> ChainService prepares the transaction
  -> App-selected SignerProvider signs or submits
  -> Portfolio service updates positions and account totals
```

The SDK can provide helpers, defaults, and policy hooks, but the consumer should
be able to override all choices.

## Product Account Shape

A product account is a user-facing container. It can reference custodial
balances, self-custody balances, offchain ledger balances, onchain positions,
and future broker positions.

```text
ProductAccount
  id
  kind
  display_name
  owner_ref
  custody_model
  supported_actions
  balance_views
  positions
  funding_routes
  signer_preferences
  policy_hints
  metadata
```

Suggested account kinds:

```text
credit_card
investment
savings_earn
trading
wallet
treasury
rewards
custom
```

Suggested custody models:

```text
self_custody
mpc_custody
custodial_ledger
broker_ledger
hybrid
```

The `kind` helps SDK consumers build standard product modules. The
`custody_model` describes where assets are actually controlled or recorded.
Neither field should force one signer implementation.

## Signing Is A Preference, Not A Product Law

Earlier examples used account abstraction for trading and MPC for large-value
transfers. That should be modeled as configurable product policy, not as a
hard-coded SDK rule.

Better shape:

```text
ProductAccount
  signer_preferences:
    default
    per_action
    per_asset
    per_threshold
```

Example:

```json
{
  "id": "savings-main",
  "kind": "savings_earn",
  "displayName": "Savings Earn",
  "custodyModel": "hybrid",
  "signerPreferences": {
    "default": { "mode": "local_key" },
    "perAction": {
      "withdraw": { "mode": "mpc", "reason": "consumer_selected_policy" }
    },
    "perThreshold": [
      {
        "assetGroupId": "usdc",
        "amount": "100000.00",
        "currency": "USD",
        "preferredSigner": { "mode": "mpc" }
      }
    ]
  }
}
```

The SDK should make this easy, but the app decides whether to apply it.

## Three Example Accounts

### Credit Card Account

Best fit: product account backed by a custodial or issuer ledger, with optional
onchain top-up and settlement routes.

```text
CreditCardAccount
  user sees: spendable card balance, transactions, limits, card controls
  under hood: custodial ledger balance + top-up routes
  signing: often none for card spend; signer used only for onchain top-up
```

Example:

```json
{
  "id": "card-primary",
  "kind": "credit_card",
  "displayName": "Credit Card",
  "custodyModel": "custodial_ledger",
  "supportedActions": ["top_up", "freeze_card", "set_limit", "view_pin"],
  "balanceViews": [
    {
      "id": "card-spendable-usd",
      "assetGroupId": "usd",
      "source": "custodial_ledger",
      "display": "unified"
    }
  ],
  "fundingRoutes": [
    {
      "id": "topup-usdc-base",
      "fromAssetGroupId": "usdc",
      "preferredAssetInstanceId": "eip155:8453/erc20:0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913",
      "toLedger": "card-spendable-usd",
      "routeKind": "onchain_to_custodial"
    }
  ],
  "signerPreferences": {
    "perAction": {
      "top_up": { "mode": "app_selected" }
    }
  }
}
```

### Investment Account

Best fit: product account that aggregates multiple instruments and rails:
native coins, fungible tokens, tokenized stocks, broker-ledger stocks, ETFs, and
future funds.

```text
InvestmentAccount
  user sees: allocation, portfolio value, holdings, trending assets
  under hood: AssetGroups and AssetInstruments across crypto and stocks
  signing: app can choose local, MPC, Privy, AA, broker auth, or custodial flow
```

Example:

```json
{
  "id": "investment-main",
  "kind": "investment",
  "displayName": "Investment Hub",
  "custodyModel": "hybrid",
  "supportedActions": ["buy", "sell", "swap", "rebalance", "withdraw"],
  "balanceViews": [
    {
      "id": "all-investments",
      "display": "unified",
      "groupBy": ["asset_class", "asset_group"]
    }
  ],
  "positions": [
    {
      "assetGroupId": "btc",
      "instrumentId": "btc.native",
      "source": "onchain_or_indexed"
    },
    {
      "assetGroupId": "tsla",
      "instrumentId": "tsla.xstock",
      "source": "tokenized_equity"
    },
    {
      "assetGroupId": "nvda",
      "instrumentId": "nvda.broker",
      "source": "broker_ledger"
    }
  ],
  "signerPreferences": {
    "default": { "mode": "app_selected" },
    "perAction": {
      "swap": { "mode": "account_abstraction", "optional": true },
      "withdraw": { "mode": "mpc", "optional": true }
    }
  }
}
```

### Savings Earn Account

Best fit: product account over yield-bearing positions such as vault shares,
lending receipts, staking positions, and liquid staked assets.

```text
SavingsEarnAccount
  user sees: earned interest, APY, vaults, deposit, withdraw
  under hood: positions can be vault shares, receipt tokens, LSTs, or ledgers
  signing: app may prefer stronger signing for withdrawal or high-value actions
```

Example:

```json
{
  "id": "savings-main",
  "kind": "savings_earn",
  "displayName": "Savings Earn",
  "custodyModel": "hybrid",
  "supportedActions": ["deposit", "withdraw", "claim_rewards"],
  "positions": [
    {
      "id": "usdc-vault-position",
      "label": "USDC Vault",
      "underlyingAssetGroupId": "usdc",
      "positionInstrumentKind": "vault_share",
      "source": "onchain"
    },
    {
      "id": "eth-staking-position",
      "label": "ETH Staking",
      "underlyingAssetGroupId": "eth",
      "positionInstrumentKind": "liquid_staked_asset",
      "source": "onchain"
    }
  ],
  "signerPreferences": {
    "default": { "mode": "local_key" },
    "perAction": {
      "withdraw": { "mode": "app_selected" }
    }
  }
}
```

## Unified And Split Views

The same account should support unified and split views.

Unified USDC view:

```text
USDC
  total: 1,250.00
  instances:
    Ethereum USDC: 500.00
    Base USDC: 750.00
```

Split operational view:

```text
Base USDC
  asset_instance_id: eip155:8453/erc20:...
  actions: transfer, approve, swap, deposit_to_vault

Ethereum USDC
  asset_instance_id: eip155:1/erc20:...
  actions: transfer, approve, bridge, deposit_to_vault
```

Product accounts should choose the default view, but the underlying model
should keep both available.

## Flutter SDK Usage Example

This is the type of end-user flow Atlas should enable after code generation.
Names are illustrative, not final API commitments.

```dart
final atlas = AtlasClient(
  registries: AtlasRegistries.bundled().withRemoteAssets(),
  signers: [
    LocalKeySigner(...),
    MpcSigner(...),
    PrivySigner(...),
    AccountAbstractionSigner(...),
  ],
);

final accounts = await atlas.productAccounts.forUser(userId);
final savings = accounts.byKind(ProductAccountKind.savingsEarn).first;

final quote = await savings.prepareDeposit(
  asset: AssetSelector.group('usdc'),
  amount: Decimal.parse('100.00'),
  preferredNetwork: NetworkId.base,
);

final result = await quote.execute(
  signer: atlas.signers.select(
    mode: SignerMode.appSelected,
    account: savings,
    action: ProductAction.deposit,
  ),
);
```

The consumer can stay high-level, while Atlas still resolves to:

```text
AssetGroup: usdc
  -> AssetInstrument: usdc.circle
      -> AssetInstance: Base USDC ERC-20
          -> ChainService: EVM/Base
              -> SignerProvider: selected by app
```

## How This Connects To Portfolio APIs

Future portfolio APIs can use product accounts as views over the same canonical
asset model.

```text
Portfolio API
  by product account
  by asset group
  by instrument
  by network
  by custody model
  by signer/security posture
```

Examples:

- Show one total USDC balance across all accounts.
- Show USDC split by Credit Card, Savings Earn, and Investment Hub.
- Show onchain USDC split by Ethereum and Base.
- Show card ledger USD separately from onchain USDC, even if USDC can top it up.
- Show stocks whether they are broker-ledger equities or tokenized equities.

This is why product accounts should reference `AssetGroup`,
`AssetInstrument`, and `AssetInstance` ids instead of inventing separate asset
objects.

## Design Rules To Keep

- Product accounts are app-facing composition, not low-level execution.
- Chain services only execute exact `AssetInstance` operations.
- Product policies are hints/defaults, not mandatory SDK behavior.
- Signing remains provider-neutral and app-selectable.
- Custodial, broker, MPC, local, Privy, and account-abstraction flows can
  coexist in the same mobile app.
- Unified views must always preserve drill-down to exact networks, instances,
  ledgers, and routes.
- Amounts, balances, prices, and rates follow `CODEX.md`: no floats for money.
- Product-account BDD tests should come later, once the core registry and asset
  model is stable.

## Deferred Follow-Up Specs

- Concrete product account API and Rust traits.
- Balance API aggregation model.
- Portfolio API query model.
- Card ledger and top-up route model.
- Broker-ledger and tokenized equity model.
- Yield position model for vaults, lending receipts, staking, and LSTs.
- Product-policy engine for action limits, signer selection, and risk scoring.
