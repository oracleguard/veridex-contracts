#!/bin/bash
set -e
echo "Running tests with coverage..."
cargo test --all --coverage
echo "✓ Coverage report generated"
