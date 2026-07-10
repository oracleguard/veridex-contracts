#!/bin/bash
set -e
echo "Building release (WASM)..."
cargo build --release --target wasm32-unknown-unknown
echo "✓ WASM release build complete"
