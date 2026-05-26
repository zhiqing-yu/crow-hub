#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

echo "=== gate 1: cargo check --all ==="
cargo check --all 2>&1 | grep -E 'error|warning' || echo "  clean"

echo ""
echo "=== gate 2: cargo test --workspace --lib ==="
cargo test --workspace --lib 2>&1 | grep 'test result' | grep -v '0 passed'

echo ""
echo "=== gate 3: cargo test -p ch-tui ==="
cargo test -p ch-tui 2>&1 | grep 'test result'

echo ""
echo "=== gate 4: no bare Color:: in app.rs ==="
grep -n 'Color::' crates/ch-tui/src/app.rs | grep -v 'app.theme' | grep -v 'theme\.' | grep -v '//' && exit 1 || echo "  clean"

echo ""
echo "=== gate 5: git status ==="
git diff --exit-code && git diff --cached --exit-code && echo "  clean" || echo "  uncommitted changes (allowed)"

echo ""
echo "=== all gates passed ==="
