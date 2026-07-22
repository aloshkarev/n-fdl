#!/usr/bin/env bash
# Wave 1 grammar conformance: both tree-sitter and Rust must parse examples
# without error. Deep CST↔AST shape comparison is deferred (TODO).
#
# Usage (from repo root or any cwd):
#   ./scripts/check_grammar_conformance.sh
#
# Requires:
#   - tree-sitter CLI on PATH (or installable via npm in grammar dirs)
#   - cargo + workspace crates (ndsl-cli)
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

NFDL_GRAMMAR="$ROOT/grammars/tree-sitter-nfdl"
ADGL_GRAMMAR="$ROOT/grammars/tree-sitter-adgl"

failures=0
checked=0

log() { printf '%s\n' "$*"; }
err() { printf 'error: %s\n' "$*" >&2; }

resolve_tree_sitter() {
  if command -v tree-sitter >/dev/null 2>&1; then
    TREE_SITTER=(tree-sitter)
    return 0
  fi
  if command -v npx >/dev/null 2>&1; then
    # Prefer grammar-local tree-sitter-cli after npm install.
    TREE_SITTER=(npx --no-install tree-sitter)
    return 0
  fi
  err "tree-sitter CLI not found (install tree-sitter-cli or ensure it is on PATH)"
  return 1
}

ensure_grammar_ready() {
  local grammar_dir="$1"
  if [[ ! -f "$grammar_dir/src/parser.c" ]]; then
    err "missing generated parser: $grammar_dir/src/parser.c (run: tree-sitter generate)"
    return 1
  fi
  # If using npx --no-install, make sure local CLI exists.
  if [[ "${TREE_SITTER[*]}" == npx* ]] && [[ ! -x "$grammar_dir/node_modules/.bin/tree-sitter" ]]; then
    log "→ npm install in $(basename "$grammar_dir") (tree-sitter-cli)"
    (cd "$grammar_dir" && npm install --no-fund --no-audit)
  fi
}

ts_parse_file() {
  local grammar_dir="$1"
  local file="$2"
  local out rc
  set +e
  out="$(
    cd "$grammar_dir" && "${TREE_SITTER[@]}" parse -q "$file" 2>&1
  )"
  rc=$?
  set -e
  if [[ $rc -ne 0 ]]; then
    err "tree-sitter parse failed: $file"
    [[ -n "$out" ]] && printf '%s\n' "$out" >&2
    return 1
  fi
  # Belt-and-suspenders: ERROR / MISSING nodes (exit may be 0 on some CLI versions).
  if printf '%s' "$out" | grep -qE '\(ERROR|\(MISSING'; then
    err "tree-sitter ERROR/MISSING nodes: $file"
    printf '%s\n' "$out" >&2
    return 1
  fi
  return 0
}

rust_parse_file() {
  local file="$1"
  local out rc
  set +e
  out="$(cargo run -q -p ndsl-cli -- parse "$file" 2>&1)"
  rc=$?
  set -e
  if [[ $rc -ne 0 ]]; then
    err "Rust (ndsl-cli) parse failed: $file"
    [[ -n "$out" ]] && printf '%s\n' "$out" >&2
    return 1
  fi
  return 0
}

check_pair() {
  local grammar_dir="$1"
  local file="$2"
  local label="$3"
  checked=$((checked + 1))
  log "[$label] $(realpath --relative-to="$ROOT" "$file" 2>/dev/null || echo "$file")"
  if ! ts_parse_file "$grammar_dir" "$file"; then
    failures=$((failures + 1))
    return 0
  fi
  if ! rust_parse_file "$file"; then
    failures=$((failures + 1))
    return 0
  fi
}

# --- main ---

if [[ ! -d "$NFDL_GRAMMAR" || ! -d "$ADGL_GRAMMAR" ]]; then
  err "expected grammars under grammars/tree-sitter-{nfdl,adgl}"
  exit 2
fi

# Expand globs safely: fail if no examples.
shopt -s nullglob
NFDL_EXAMPLES=("$ROOT"/docs/examples/*.nfdl)
ADGL_EXAMPLES=("$ROOT"/docs/idea/examples/*.adgl)
shopt -u nullglob

if [[ ${#NFDL_EXAMPLES[@]} -eq 0 ]]; then
  err "no docs/examples/*.nfdl files found"
  exit 2
fi
if [[ ${#ADGL_EXAMPLES[@]} -eq 0 ]]; then
  err "no docs/idea/examples/*.adgl files found"
  exit 2
fi

resolve_tree_sitter
ensure_grammar_ready "$NFDL_GRAMMAR"
ensure_grammar_ready "$ADGL_GRAMMAR"

log "Grammar conformance (Wave 1: both sides parse without error)"
log "tree-sitter: ${TREE_SITTER[*]}"
log "Rust: cargo run -q -p ndsl-cli -- parse <file>"
log "TODO: deep CST↔AST shape comparison in a later wave"
log ""

for f in "${NFDL_EXAMPLES[@]}"; do
  check_pair "$NFDL_GRAMMAR" "$f" "nfdl"
done

for f in "${ADGL_EXAMPLES[@]}"; do
  check_pair "$ADGL_GRAMMAR" "$f" "adgl"
done

log ""
if [[ $failures -ne 0 ]]; then
  err "conformance failed: $failures/$checked example(s) unparseable on at least one side"
  exit 1
fi

log "ok: $checked examples parse on both tree-sitter and Rust"
exit 0
