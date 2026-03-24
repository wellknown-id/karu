#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WASM_DIR="$SCRIPT_DIR/wasm"

mkdir -p "$WASM_DIR"

# Ensure wasm target is installed
rustup target add wasm32-wasip1 2>/dev/null || true

echo "=== Building Karu WASM harness ==="
cd "$SCRIPT_DIR/karu-wasm-harness"
cargo build --release --target wasm32-wasip1
cp "$SCRIPT_DIR/karu-wasm-harness/target/wasm32-wasip1/release/karu_wasm_harness.wasm" "$WASM_DIR/karu.wasm"
echo "✓ karu.wasm ($(du -h "$WASM_DIR/karu.wasm" | cut -f1))"

echo ""
echo "=== Building Cedar WASM harness ==="
cd "$SCRIPT_DIR/cedar-wasm-harness"
cargo build --release --target wasm32-wasip1
cp "$SCRIPT_DIR/cedar-wasm-harness/target/wasm32-wasip1/release/cedar_wasm_harness.wasm" "$WASM_DIR/cedar.wasm"
echo "✓ cedar.wasm ($(du -h "$WASM_DIR/cedar.wasm" | cut -f1))"

echo ""
echo "=== Done ==="
ls -lh "$WASM_DIR"/*.wasm
echo ""
echo "Run benchmarks: cd $SCRIPT_DIR && cargo bench"
