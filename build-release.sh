#!/bin/bash
set -e
echo "Building release..."
cargo build --release
echo "✓ Release build complete"
