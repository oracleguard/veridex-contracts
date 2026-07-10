#!/bin/bash
set -e
echo "Running tests..."
cargo test --all
echo "✓ Tests complete"
