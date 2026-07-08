# veridex-contracts

> Soroban smart contracts powering the [Veridex](https://github.com/oracleguard/veridex-core) prediction markets platform on Stellar.

[![CI](https://github.com/oracleguard/veridex-contracts/actions/workflows/ci.yml/badge.svg)](https://github.com/oracleguard/veridex-contracts/actions/workflows/ci.yml)
[![License: Apache-2.0](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](LICENSE)

---

## Overview

This repository contains two Soroban contracts:

| Contract | Crate | Purpose |
|---|---|---|
| **LedgerLens Score** | `contracts/ledgerlens-score` | Risk registry storing wallet/asset-pair fraud scores from the LedgerLens off-chain engine |
| **Veridex Oracle** | `contracts/veridex-oracle` | Prediction market lifecycle: creation, staking, outcome resolution, and winnings distribution |

Both contracts are compiled to WASM and deployed on-chain. The off-chain [veridex-core](https://github.com/oracleguard/veridex-core) engine feeds data into these contracts.

---

## Repository Structure

```
veridex-contracts/
├── Cargo.toml                         # Workspace manifest
├── Makefile                           # Build / test / deploy tasks
├── rust-toolchain.toml                # Pinned Rust toolchain
├── README.md
├── DEPLOYMENT.md
├── .github/
│   └── workflows/
│       └── ci.yml                     # GitHub Actions CI pipeline
└── contracts/
    ├── ledgerlens-score/
    │   ├── Cargo.toml
    │   └── src/lib.rs                 # LedgerLens Score contract + tests
    └── veridex-oracle/
        ├── Cargo.toml
        └── src/lib.rs                 # Veridex Oracle contract + tests
```

---

## Contracts

### `ledgerlens-score`

Stores risk/fraud scores produced by the [LedgerLens](https://github.com/oracleguard/veridex-core) off-chain engine (Benford's Law, Tarjan SCC ring analysis, causal AI ensemble).

#### Data Model

```rust
pub struct RiskScore {
    pub wallet: Address,        // Scored wallet
    pub asset_pair: String,     // e.g. "XLM/USDC"
    pub score: u32,             // Risk score (0–10000 bps)
    pub confidence: u32,        // Confidence (0–10000 bps)
    pub submitted_at: u64,      // Ledger timestamp
    pub ledger_sequence: u32,   // Ledger sequence (audit trail)
}
```

Scores are expressed in **basis points** (0–10000) for safe fixed-point arithmetic. A score of `7500` means 75% risk.

#### Interface

| Function | Auth required | Description |
|---|---|---|
| `initialize(admin)` | — | One-time setup; sets oracle admin |
| `admin()` | — | Returns current admin address |
| `set_admin(new_admin)` | Admin | Transfers admin authority |
| `submit_score(wallet, asset_pair, score, confidence)` | Admin | Upsert a risk score entry |
| `get_score(wallet, asset_pair)` | — | Returns `Option<RiskScore>` |
| `get_score_value(wallet, asset_pair)` | — | Returns `Option<u32>` (score bps only) |
| `has_score(wallet, asset_pair)` | — | Returns `bool` |

#### Example — Submit a score

```bash
stellar contract invoke \
  --id <LEDGERLENS_CONTRACT_ID> \
  --source oracle-key \
  --network testnet \
  -- submit_score \
  --wallet GABC...XYZ \
  --asset_pair "XLM/USDC" \
  --score 7500 \
  --confidence 9200
```

#### Example — Query a score

```bash
stellar contract invoke \
  --id <LEDGERLENS_CONTRACT_ID> \
  --network testnet \
  -- get_score \
  --wallet GABC...XYZ \
  --asset_pair "XLM/USDC"
```

---

### `veridex-oracle`

Manages the full lifecycle of Veridex prediction markets: creation, staking, oracle resolution, and pro-rata winnings distribution.

#### Market Lifecycle

```
create_market()
      │
      ▼
  [Open] ◄─── stake(market_id, participant, outcome, amount)
      │
      ├─── resolve_market(market_id, outcome)  →  [Resolved]
      │                                               │
      │                                               └─► claim_winnings()
      │
      └─── void_market(market_id)  →  [Voided]
                                           │
                                           └─► claim_winnings()  (refund)
```

#### Data Model

```rust
pub struct MarketState {
    pub market_id: u64,
    pub description: String,
    pub token: Address,           // SAC token for stakes
    pub close_time: u64,          // Staking deadline (ledger timestamp)
    pub status: MarketStatus,     // Open | Resolved | Voided
    pub outcome_count: u32,       // Number of discrete outcomes
    pub winning_outcome: Option<u32>,
    pub outcome_stakes: Vec<i128>,// Per-outcome staked totals (stroops)
    pub total_stake: i128,        // Total staked (stroops)
    pub resolved_at: Option<u32>, // Ledger sequence at resolution
}
```

#### Interface

| Function | Auth required | Description |
|---|---|---|
| `initialize(admin)` | — | One-time setup |
| `admin()` | — | Returns admin address |
| `set_admin(new_admin)` | Admin | Transfer authority |
| `create_market(description, token, close_time, outcome_count)` | Admin | Create a new market, returns `market_id` |
| `stake(market_id, participant, outcome, amount)` | Participant | Stake tokens on an outcome |
| `resolve_market(market_id, outcome)` | Admin | Declare winning outcome |
| `void_market(market_id)` | Admin | Void market; enables full refunds |
| `claim_winnings(market_id, participant)` | Participant | Collect winnings or refund |
| `get_market_state(market_id)` | — | Read full `MarketState` |
| `get_stake(market_id, participant)` | — | Read participant's stake |
| `next_id()` | — | Next market ID to be assigned |

#### Example — Create a market

```bash
stellar contract invoke \
  --id <ORACLE_CONTRACT_ID> \
  --source oracle-key \
  --network testnet \
  -- create_market \
  --description "Will XLM hit $1 by 2027-01-01?" \
  --token <TOKEN_CONTRACT_ID> \
  --close_time 1767225600 \
  --outcome_count 2
```

#### Example — Resolve a market

```bash
stellar contract invoke \
  --id <ORACLE_CONTRACT_ID> \
  --source oracle-key \
  --network testnet \
  -- resolve_market \
  --market_id 0 \
  --outcome 1
```

#### Example — Claim winnings

```bash
stellar contract invoke \
  --id <ORACLE_CONTRACT_ID> \
  --source winner-key \
  --network testnet \
  -- claim_winnings \
  --market_id 0 \
  --participant GABC...XYZ
```

---

## Getting Started

### Prerequisites

- [Rust](https://rustup.rs) stable (`1.81.0` pinned via `rust-toolchain.toml`)
- `wasm32-unknown-unknown` target: `rustup target add wasm32-unknown-unknown`
- [Stellar CLI](https://developers.stellar.org/docs/tools/developer-tools/cli/install): `cargo install --locked stellar-cli --features opt`

### Build

```bash
make build
# WASM artifacts in target/wasm32-unknown-unknown/release/
```

### Test

```bash
make test
```

### Lint

```bash
make lint
make fmt
```

---

## Security

- **Admin-only writes**: `submit_score` and `resolve_market` require authorization from the designated admin address.
- **Single resolution**: Markets can only be resolved once; double-resolution panics.
- **Idempotent claims**: After claiming, the stake entry is zeroed; double-claiming panics.
- **Basis-point validation**: Score and confidence inputs are validated to `[0, 10000]`.
- **Overflow safety**: Payout arithmetic uses checked integer operations.
- **TTL management**: Persistent storage entries are extended to ~1 year of ledger closings on write.

---

## Integration with veridex-core

The off-chain [veridex-core](https://github.com/oracleguard/veridex-core) engine:

1. Runs Benford's Law + Tarjan SCC + causal AI analysis on market participants.
2. Calls `submit_score` on `ledgerlens-score` with the resulting risk scores.
3. Monitors markets and calls `resolve_market` or `void_market` on `veridex-oracle` when outcomes are determined.

---

## License

Apache-2.0 © Veridex Contributors
