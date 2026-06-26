# N-FDL v1 — Network Format Description Language

N-FDL is a declarative language for describing binary network protocols + an execution engine (bytecode VM + EFSM) that can parse, validate, and drive state machines over real traffic.

## Key Capabilities (Current)

- **Parser**: Full support for fields, dependent lengths (`bytes[length-2]`), `let` bindings, `__current_offset`, `loop ... while (expr)`, `MessageRef` (inline structs), and `state_machine { ... on Msg guard (...) -> State { set ...; emit ...; } }`.
- **Bytecode VM**: Generates and executes compact bytecode. Supports loops via jumps, arithmetic/boolean expressions, dynamic lengths.
- **Runner**: Executes protocols end-to-end. Builds rich context from fields/lets/loops. Integrates with EFSM.
- **EFSM**: Real guard evaluation + action execution (`set` updates flow variables, `emit` produces events). State transitions (e.g. IDLE → PENDING on RADIUS `code == 1`).
- **Safety**: Instruction limits, recursion depth limits for MessageRef, context size limits.
- **Examples**: Full working support for `radius.nfdl` (with loops + Attribute refs + state machine) and others.

See `PRODUCTION_CHECKLIST.md` for detailed checklist and roadmap.

## Quick Start

```bash
# Build & run (example using nfdl-cli if available)
cargo build -p nfdl-cli

# Parse and inspect
cargo run -p nfdl-cli -- radius.nfdl

# Run tests (including real radius e2e)
cargo test -p nfdl-syntax
cargo test -p nfdl-runtime --test fsm_integration -- --nocapture
```

## Example: RADIUS state machine (excerpt from radius.nfdl)

```nfdl
let attrs_len = length - 20;
let start_offset = __current_offset;

loop attrs
    while (__current_offset - start_offset) < attrs_len
{
    attr: Attribute;
}

state_machine AuthDialog {
    key = (bidir(UDP.src_port, UDP.dst_port), identifier);

    state IDLE {
        on AccessMessage guard (code == 1) -> PENDING {
            set req_auth = authenticator;
        };
    }
    state PENDING { ... }
}
```

The engine will:
1. Execute the loop + MessageRef inlining via bytecode.
2. Build context with `code`, `authenticator`, `attrs_len`, etc.
3. Evaluate guard → fire transition + execute `set`.
4. Return final state + events.

## Architecture Highlights

- **Lexer + Parser** (`nfdl-syntax`): Real recursive descent, supports modern NFDL syntax.
- **Bytecode** (`nfdl-runtime`): `Instruction` + `BytecodeVm` (IP execution, no interpretation overhead in hot path).
- **Runner + EFSM**: `parse_and_run_with_data` → VM → ctx → `FsmEngine::feed`.
- **Limits & Errors**: `RuntimeError::LimitExceeded`, depth guards, etc. (improving in v1.5).
- **No unsafe** in core crates.

## Production v1 Progress

Core execution pipeline (parser → bytecode → VM → rich ctx → EFSM actions) is functional and tested on real protocols.


## Roadmap Summary

- **v1 (current)**: Core parser + VM + EFSM + basic limits + state machines.
- **v1.5**: Proper error model, safety hardening, test coverage, multi-message support.
- **v2**: Production features (reassembly integration, fuzz campaigns, formal methods, high performance).

## Contributing / Running

- Workspace: `nfdl-syntax`, `nfdl-runtime`, `nfdl-verify`, `nfdl-fuzz`, `nfdl-cli`.
- Focused tests: `fsm_integration` for full pipeline.
- Always export cargo path if needed: `export PATH="$HOME/.cargo/bin:$PATH"`

For detailed status and next steps, see `PRODUCTION_CHECKLIST.md`.

**Ready for research use and incremental production hardening.**
