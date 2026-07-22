#!/usr/bin/env bash
# Local release gate: same four checks as CI (.github/workflows/ci.yml).
#
# Dual-track reminder: grammar conformance covers the tree-sitter IDE track
# (Rust ↔ editors). It must not feed verify or runtime.
#
# Usage (from repo root or any cwd):
#   ./scripts/release-gate.sh
#   ./scripts/release-gate.sh --help
#   ./scripts/release-gate.sh --dry-run   # print steps only
#
# Exits non-zero on the first failing step.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

usage() {
  cat <<'EOF'
Usage: ./scripts/release-gate.sh [--help] [--dry-run]

Run the four CI release gates from the n-fdl repo root (fail-fast):

  1. cargo fmt --all -- --check
  2. cargo clippy --workspace -- -D warnings
  3. cargo test --workspace
  4. ./scripts/check_grammar_conformance.sh

Options:
  -h, --help     Show this help and exit
  --dry-run      Print step banners only; do not run commands

Exit status is non-zero if any gate fails (or on usage errors).
EOF
}

DRY_RUN=0
for arg in "$@"; do
  case "$arg" in
    -h|--help)
      usage
      exit 0
      ;;
    --dry-run)
      DRY_RUN=1
      ;;
    *)
      printf 'error: unknown argument: %s\n\n' "$arg" >&2
      usage >&2
      exit 2
      ;;
  esac
done

banner() {
  local step="$1"
  local title="$2"
  printf '\n======== [%s/4] %s ========\n' "$step" "$title"
}

run_step() {
  local step="$1"
  local title="$2"
  shift 2
  banner "$step" "$title"
  if [[ "$DRY_RUN" -eq 1 ]]; then
    printf 'dry-run: %s\n' "$*"
    return 0
  fi
  "$@"
}

run_step 1 "cargo fmt --check" \
  cargo fmt --all -- --check

run_step 2 "cargo clippy (-D warnings)" \
  cargo clippy --workspace -- -D warnings

run_step 3 "cargo test --workspace" \
  cargo test --workspace

run_step 4 "grammar conformance" \
  ./scripts/check_grammar_conformance.sh

printf '\n======== release-gate: all %s ========\n' \
  "$([[ "$DRY_RUN" -eq 1 ]] && echo 'steps listed (dry-run)' || echo 'gates passed')"
exit 0
