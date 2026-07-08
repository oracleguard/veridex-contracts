# Deployment Guide

Complete instructions for deploying `ledgerlens-score` and `veridex-oracle` to Stellar testnet and mainnet.

---

## Table of Contents

1. [Prerequisites](#prerequisites)
2. [Environment Setup](#environment-setup)
3. [Testnet Deployment](#testnet-deployment)
4. [Mainnet Deployment](#mainnet-deployment)
5. [Post-Deployment Initialization](#post-deployment-initialization)
6. [Contract Upgrade](#contract-upgrade)
7. [Verification](#verification)
8. [Troubleshooting](#troubleshooting)

---

## Prerequisites

### Required Tools

```bash
# Install Rust toolchain
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup target add wasm32-unknown-unknown
rustup component add rustfmt clippy

# Install Stellar CLI
cargo install --locked stellar-cli --features opt

# Verify installation
stellar --version
cargo --version
```

### Required Accounts

- A **deployer account** with sufficient XLM to cover contract deployment fees.
- An **admin/oracle account** — this address will be passed to `initialize()` and becomes the oracle authority.
  - On testnet you can fund with Friendbot.
  - On mainnet ensure the account holds at least 2 XLM for reserves.

---

## Environment Setup

### Generate / Import Keys

```bash
# Generate a new key (testnet only — never do this for mainnet keys)
stellar keys generate --global deployer --network testnet

# Import an existing secret key
stellar keys add --global deployer --secret-key

# Show the public key
stellar keys address deployer
```

### Fund Testnet Account

```bash
stellar network use testnet
stellar keys fund deployer --network testnet
```

### Verify Balance

```bash
stellar account balances --address $(stellar keys address deployer) --network testnet
```

---

## Testnet Deployment

### 1. Build Contracts

```bash
make build
```

Output WASMs will be at:
- `target/wasm32-unknown-unknown/release/ledgerlens_score.wasm`
- `target/wasm32-unknown-unknown/release/veridex_oracle.wasm`

### 2. Deploy ledgerlens-score

```bash
LEDGERLENS_CONTRACT=$(stellar contract deploy \
  --wasm target/wasm32-unknown-unknown/release/ledgerlens_score.wasm \
  --source deployer \
  --network testnet)

echo "ledgerlens-score: $LEDGERLENS_CONTRACT"
```

### 3. Deploy veridex-oracle

```bash
ORACLE_CONTRACT=$(stellar contract deploy \
  --wasm target/wasm32-unknown-unknown/release/veridex_oracle.wasm \
  --source deployer \
  --network testnet)

echo "veridex-oracle: $ORACLE_CONTRACT"
```

### 4. Initialize Both Contracts

```bash
ADMIN_ADDRESS=$(stellar keys address deployer)

# Initialize ledgerlens-score
stellar contract invoke \
  --id $LEDGERLENS_CONTRACT \
  --source deployer \
  --network testnet \
  -- initialize \
  --admin $ADMIN_ADDRESS

# Initialize veridex-oracle
stellar contract invoke \
  --id $ORACLE_CONTRACT \
  --source deployer \
  --network testnet \
  -- initialize \
  --admin $ADMIN_ADDRESS
```

### 5. Verify Deployment

```bash
# Should return the admin address
stellar contract invoke \
  --id $LEDGERLENS_CONTRACT \
  --network testnet \
  -- admin

stellar contract invoke \
  --id $ORACLE_CONTRACT \
  --network testnet \
  -- admin
```

### Using the Makefile

```bash
# Build + deploy in one step
make deploy-testnet DEPLOYER_KEY=deployer

# Initialize (requires contract IDs from previous step)
make init-testnet \
  ADMIN_ADDRESS=G... \
  LEDGERLENS_CONTRACT=C... \
  ORACLE_CONTRACT=C...
```

---

## Mainnet Deployment

> **Warning**: Mainnet deployments use real XLM. Double-check all parameters before proceeding.

### Pre-Deployment Checklist

- [ ] Contracts tested thoroughly on testnet
- [ ] Admin key is a hardware wallet or multi-sig
- [ ] Admin key has sufficient XLM balance (≥ 10 XLM recommended)
- [ ] WASM hashes have been audited
- [ ] Contract source code matches the deployed WASM (use `stellar contract info --wasm-hash`)

### 1. Set Up Mainnet Key

```bash
# Import your mainnet hardware/cold wallet key
stellar keys add --global mainnet-deployer --secret-key
# Enter your secret key when prompted (starts with S...)
```

### 2. Build for Mainnet

```bash
make build
```

### 3. Deploy

```bash
# Deploy ledgerlens-score
LEDGERLENS_MAINNET=$(stellar contract deploy \
  --wasm target/wasm32-unknown-unknown/release/ledgerlens_score.wasm \
  --source mainnet-deployer \
  --network mainnet \
  --rpc-url https://rpc.mainnet.stellar.gateway.fm \
  --network-passphrase "Public Global Stellar Network ; September 2015")

echo "ledgerlens-score mainnet: $LEDGERLENS_MAINNET"

# Deploy veridex-oracle
ORACLE_MAINNET=$(stellar contract deploy \
  --wasm target/wasm32-unknown-unknown/release/veridex_oracle.wasm \
  --source mainnet-deployer \
  --network mainnet \
  --rpc-url https://rpc.mainnet.stellar.gateway.fm \
  --network-passphrase "Public Global Stellar Network ; September 2015")

echo "veridex-oracle mainnet: $ORACLE_MAINNET"
```

### 4. Initialize

```bash
MAINNET_ADMIN=<your-mainnet-admin-address>

stellar contract invoke \
  --id $LEDGERLENS_MAINNET \
  --source mainnet-deployer \
  --network mainnet \
  -- initialize \
  --admin $MAINNET_ADMIN

stellar contract invoke \
  --id $ORACLE_MAINNET \
  --source mainnet-deployer \
  --network mainnet \
  -- initialize \
  --admin $MAINNET_ADMIN
```

Or via Makefile:

```bash
make deploy-mainnet DEPLOYER_KEY=mainnet-deployer
```

---

## Post-Deployment Initialization

### LedgerLens Score — Submit Initial Scores

```bash
stellar contract invoke \
  --id $LEDGERLENS_CONTRACT \
  --source oracle-key \
  --network testnet \
  -- submit_score \
  --wallet G... \
  --asset_pair "XLM/USDC" \
  --score 2500 \
  --confidence 8000
```

### Veridex Oracle — Create First Market

```bash
stellar contract invoke \
  --id $ORACLE_CONTRACT \
  --source oracle-key \
  --network testnet \
  -- create_market \
  --description "Will XLM reach $0.50 by 2027-01-01?" \
  --token <USDC_SAC_ADDRESS> \
  --close_time 1767225600 \
  --outcome_count 2
```

---

## Contract Upgrade

Soroban contracts support WASM upgrades via the `update_current_contract_wasm` host function. To upgrade:

```bash
# 1. Build new WASM
make build

# 2. Upload new WASM to the network (returns wasm_hash)
WASM_HASH=$(stellar contract upload \
  --wasm target/wasm32-unknown-unknown/release/ledgerlens_score.wasm \
  --source deployer \
  --network testnet)

# 3. Invoke the upgrade function on the deployed contract
#    (Requires upgrade entrypoint in contract — add as needed)
stellar contract invoke \
  --id $LEDGERLENS_CONTRACT \
  --source deployer \
  --network testnet \
  -- upgrade \
  --new_wasm_hash $WASM_HASH
```

---

## Verification

### Verify WASM Hash (Source Integrity)

```bash
# Compute local hash
stellar contract inspect --wasm target/wasm32-unknown-unknown/release/ledgerlens_score.wasm

# Compare with on-chain hash
stellar contract info --id $LEDGERLENS_CONTRACT --network testnet
```

### Monitor on Stellar Expert

- Testnet: `https://stellar.expert/explorer/testnet/contract/<CONTRACT_ID>`
- Mainnet: `https://stellar.expert/explorer/public/contract/<CONTRACT_ID>`

---

## Troubleshooting

| Error | Cause | Fix |
|---|---|---|
| `insufficient funds` | Deployer account low on XLM | Fund via Friendbot (testnet) or transfer XLM |
| `already initialized` | `initialize()` called twice | Skip initialization; contract is already ready |
| `contract not initialized` | Admin lookup before `initialize()` | Call `initialize(admin)` first |
| `score exceeds 10000 bps` | Score value > 10000 | Use basis-point values (0–10000) |
| `market not found` | Bad `market_id` | Check `next_id()` to see valid range |
| `staking period has closed` | `close_time` passed | Create a new market with future `close_time` |
| `wasm32 target missing` | Rust target not installed | `rustup target add wasm32-unknown-unknown` |

---

## Contract Addresses

Update this table after each deployment:

| Network | Contract | Address |
|---|---|---|
| Testnet | ledgerlens-score | `TBD` |
| Testnet | veridex-oracle | `TBD` |
| Mainnet | ledgerlens-score | `TBD` |
| Mainnet | veridex-oracle | `TBD` |
