# ===========================================================================
# Veridex Contracts — Makefile
# ===========================================================================
# Targets:
#   make build       — compile all contracts to WASM (release)
#   make test        — run unit + integration tests
#   make fmt         — format all Rust source
#   make lint        — clippy lints
#   make clean       — remove build artifacts
#   make deploy-testnet  — deploy both contracts to Stellar testnet
#   make deploy-mainnet  — deploy both contracts to Stellar mainnet (guarded)
#   make check-tools     — verify required tooling is installed
# ===========================================================================

SHELL := /bin/bash
.PHONY: all build test fmt lint clean deploy-testnet deploy-mainnet check-tools \
        build-ledgerlens build-oracle install-soroban

# ---------------------------------------------------------------------------
# Variables
# ---------------------------------------------------------------------------
SOROBAN   ?= stellar contract
CARGO     ?= cargo
NETWORK_TESTNET  ?= testnet
NETWORK_MAINNET  ?= mainnet
RPC_TESTNET      ?= https://soroban-testnet.stellar.org
RPC_MAINNET      ?= https://rpc.mainnet.stellar.gateway.fm
DEPLOYER_KEY     ?= deployer   # stellar key name from ~/.config/stellar/

WASM_DIR   := target/wasm32-unknown-unknown/release
LL_WASM    := $(WASM_DIR)/ledgerlens_score.wasm
ORACLE_WASM:= $(WASM_DIR)/veridex_oracle.wasm

# ---------------------------------------------------------------------------
# Default
# ---------------------------------------------------------------------------
all: build

# ---------------------------------------------------------------------------
# Tooling check
# ---------------------------------------------------------------------------
check-tools:
	@echo "==> Checking required tools..."
	@command -v stellar >/dev/null 2>&1 || { echo "ERROR: 'stellar' CLI not found. Install: https://developers.stellar.org/docs/tools/developer-tools/cli/install"; exit 1; }
	@command -v cargo >/dev/null 2>&1   || { echo "ERROR: 'cargo' not found. Install rustup: https://rustup.rs"; exit 1; }
	@rustup target list --installed 2>/dev/null | grep -q wasm32-unknown-unknown || { echo "ERROR: wasm32 target missing. Run: rustup target add wasm32-unknown-unknown"; exit 1; }
	@echo "    cargo: $$(cargo --version)"
	@echo "    stellar: $$(stellar --version)"
	@echo "    rustup targets: wasm32-unknown-unknown ✓"

# ---------------------------------------------------------------------------
# Install Soroban CLI (via stellar-cli)
# ---------------------------------------------------------------------------
install-soroban:
	cargo install --locked stellar-cli --features opt

# ---------------------------------------------------------------------------
# Build
# ---------------------------------------------------------------------------
build: build-ledgerlens build-oracle

build-ledgerlens:
	@echo "==> Building ledgerlens-score..."
	$(CARGO) build \
	    --manifest-path contracts/ledgerlens-score/Cargo.toml \
	    --target wasm32-unknown-unknown \
	    --release \
	    --no-default-features
	@echo "    WASM: $(LL_WASM)"

build-oracle:
	@echo "==> Building veridex-oracle..."
	$(CARGO) build \
	    --manifest-path contracts/veridex-oracle/Cargo.toml \
	    --target wasm32-unknown-unknown \
	    --release \
	    --no-default-features
	@echo "    WASM: $(ORACLE_WASM)"

# ---------------------------------------------------------------------------
# Test
# ---------------------------------------------------------------------------
test:
	@echo "==> Running all contract tests..."
	$(CARGO) test --workspace -- --nocapture

test-ledgerlens:
	@echo "==> Testing ledgerlens-score..."
	$(CARGO) test --manifest-path contracts/ledgerlens-score/Cargo.toml -- --nocapture

test-oracle:
	@echo "==> Testing veridex-oracle..."
	$(CARGO) test --manifest-path contracts/veridex-oracle/Cargo.toml -- --nocapture

# ---------------------------------------------------------------------------
# Code quality
# ---------------------------------------------------------------------------
fmt:
	@echo "==> Formatting..."
	$(CARGO) fmt --all

fmt-check:
	@echo "==> Checking formatting..."
	$(CARGO) fmt --all -- --check

lint:
	@echo "==> Running clippy..."
	$(CARGO) clippy --all-targets --all-features -- -D warnings

# ---------------------------------------------------------------------------
# Clean
# ---------------------------------------------------------------------------
clean:
	@echo "==> Cleaning build artifacts..."
	$(CARGO) clean

# ---------------------------------------------------------------------------
# Deploy — Testnet
# ---------------------------------------------------------------------------
deploy-testnet: build
	@echo "==> Deploying to Stellar testnet..."
	@echo "    Deploying ledgerlens-score..."
	stellar contract deploy \
	    --wasm $(LL_WASM) \
	    --source $(DEPLOYER_KEY) \
	    --network $(NETWORK_TESTNET) \
	    --rpc-url $(RPC_TESTNET) \
	    | tee /tmp/ledgerlens-testnet-id.txt
	@echo "    ledgerlens-score contract ID: $$(cat /tmp/ledgerlens-testnet-id.txt)"

	@echo "    Deploying veridex-oracle..."
	stellar contract deploy \
	    --wasm $(ORACLE_WASM) \
	    --source $(DEPLOYER_KEY) \
	    --network $(NETWORK_TESTNET) \
	    --rpc-url $(RPC_TESTNET) \
	    | tee /tmp/oracle-testnet-id.txt
	@echo "    veridex-oracle contract ID: $$(cat /tmp/oracle-testnet-id.txt)"

# ---------------------------------------------------------------------------
# Deploy — Mainnet (guarded)
# ---------------------------------------------------------------------------
deploy-mainnet: build
	@echo ""
	@echo "!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!"
	@echo "!! WARNING: Deploying to MAINNET. This uses real XLM.          !!"
	@echo "!! Set DEPLOYER_KEY env var to your mainnet key name.          !!"
	@echo "!! Press Ctrl-C within 10s to cancel.                          !!"
	@echo "!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!"
	@echo ""
	@sleep 10

	@echo "==> Deploying ledgerlens-score to mainnet..."
	stellar contract deploy \
	    --wasm $(LL_WASM) \
	    --source $(DEPLOYER_KEY) \
	    --network $(NETWORK_MAINNET) \
	    --rpc-url $(RPC_MAINNET) \
	    | tee /tmp/ledgerlens-mainnet-id.txt

	@echo "==> Deploying veridex-oracle to mainnet..."
	stellar contract deploy \
	    --wasm $(ORACLE_WASM) \
	    --source $(DEPLOYER_KEY) \
	    --network $(NETWORK_MAINNET) \
	    --rpc-url $(RPC_MAINNET) \
	    | tee /tmp/oracle-mainnet-id.txt

	@echo "==> Mainnet deployment complete."
	@echo "    ledgerlens-score: $$(cat /tmp/ledgerlens-mainnet-id.txt)"
	@echo "    veridex-oracle:   $$(cat /tmp/oracle-mainnet-id.txt)"

# ---------------------------------------------------------------------------
# Initialize contracts (after deploy)
# ---------------------------------------------------------------------------
init-testnet:
ifndef ADMIN_ADDRESS
	$(error ADMIN_ADDRESS is not set. Usage: make init-testnet ADMIN_ADDRESS=G...)
endif
ifndef LEDGERLENS_CONTRACT
	$(error LEDGERLENS_CONTRACT is not set)
endif
ifndef ORACLE_CONTRACT
	$(error ORACLE_CONTRACT is not set)
endif
	@echo "==> Initializing ledgerlens-score on testnet..."
	stellar contract invoke \
	    --id $(LEDGERLENS_CONTRACT) \
	    --source $(DEPLOYER_KEY) \
	    --network $(NETWORK_TESTNET) \
	    --rpc-url $(RPC_TESTNET) \
	    -- initialize \
	    --admin $(ADMIN_ADDRESS)

	@echo "==> Initializing veridex-oracle on testnet..."
	stellar contract invoke \
	    --id $(ORACLE_CONTRACT) \
	    --source $(DEPLOYER_KEY) \
	    --network $(NETWORK_TESTNET) \
	    --rpc-url $(RPC_TESTNET) \
	    -- initialize \
	    --admin $(ADMIN_ADDRESS)
	@echo "==> Initialization complete."
