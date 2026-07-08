# Implementation Guide

## Step 1: Setup
```bash
cargo build --release
```

## Step 2: Test
```bash
cargo test
```

## Step 3: Deploy
```bash
soroban contract deploy --wasm target/wasm32-unknown-unknown/release/contract.wasm
```
