//! Predicate opcodes over slot registers per
//! `docs/idea/spec/06-ir-bytecode.md` §4 (Intent Bytecode, LOAD/EXPR/TOPO/WIN
//! groups) and §6 (zero-copy: `field_idx: u16`, `func_idx: u8`, slot registers
//! stack-allocated — `07-runtime.md` §8).
//!
//! Only the *shape* is defined here; interpretation (checked `i64` arithmetic,
//! Kleene `T3` logic, topology calls) is owned by the evaluator crate
//! (`07-runtime.md` §1). The EMIT/CHECK/YIELD opcode groups of `06` §4 are
//! represented structurally by [`crate::Intent`] / [`crate::ProvKey`] /
//! [`crate::PendingMatch`] rather than as opcodes.

use airpulse_dsl_types::DurationMs;

/// Number of slot registers available to one predicate program.
///
/// `07-runtime.md` §8: "Predicate bytecode — slot-registers (stack-allocated
/// array, no heap)". A fixed bound lets the evaluator keep the register file
/// in a stack array.
pub const MAX_SLOTS: usize = 16;

/// Index of a slot register, always `< MAX_SLOTS`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SlotIdx(u8);

impl SlotIdx {
    /// Constructs a slot index; `None` when `raw >= MAX_SLOTS` (no panic on
    /// data paths, `07-runtime.md` §9).
    #[must_use]
    pub const fn new(raw: u8) -> Option<SlotIdx> {
        if (raw as usize) < MAX_SLOTS {
            Some(SlotIdx(raw))
        } else {
            None
        }
    }

    /// Raw index, guaranteed `< MAX_SLOTS`.
    #[must_use]
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

/// Index of an anchor/correlate binding within one rule: the anchor is
/// binding 0, correlates follow in declaration order (`06` §2.1 ordering;
/// symbolic names live in [`crate::AnchorSpec`] / [`crate::CorrelateSpec`]).
///
/// `06` §9 bounds correlate blocks per rule to 8 (`05-verification.md` §9),
/// so `u8` is ample.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BindingIdx(pub u8);

/// Catalog field index within an event/cause/problem schema
/// (`06-ir-bytecode.md` §6: "Metric-paths — `field_idx` (u16) in opcodes, no
/// string lookup in hot-path"). Assigned by catalog resolution
/// (`05-verification.md` §1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FieldIdx(pub u16);

/// Catalog topology function index (`06-ir-bytecode.md` §6: "Topology
/// functions — `func_idx` (u8)"; signatures checked per `05-verification.md`
/// §5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TopoFuncIdx(pub u8);

/// One predicate opcode (`06-ir-bytecode.md` §4, LOAD/EXPR/TOPO/WIN groups).
///
/// Operand conventions:
/// - arithmetic is `i64` *checked* — overflow surfaces as
///   `CorrelateError::ArithOverflow` in the evaluator, never a panic
///   (`06` §8 item 6, `07` §9);
/// - `And`/`Or`/`Not` are Kleene over `T3` (`03-semantics.md` §3.7);
/// - comparisons produce `Bool` lifted to `T3` (`03` §3.7).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PredOp {
    // ── LOAD group (06 §4) ──────────────────────────────────────────────
    /// `LOAD_EVENT_FIELD binding, field_idx -> slot` — e.g.
    /// `rtx.segment_size`, `rtx.time`.
    LoadEventField {
        /// Event binding (anchor or event-correlate).
        binding: BindingIdx,
        /// Catalog field within the event schema.
        field: FieldIdx,
        /// Destination slot.
        dst: SlotIdx,
    },
    /// `LOAD_CAUSE_FIELD binding, field_idx -> slot` — e.g. `c.confidence`
    /// in a decision Cause-anchor predicate (`04-type-system.md` §3).
    LoadCauseField {
        /// Cause binding.
        binding: BindingIdx,
        /// Catalog field within the cause schema.
        field: FieldIdx,
        /// Destination slot.
        dst: SlotIdx,
    },
    /// `LOAD_PROBLEM_FIELD binding, field_idx -> slot` — e.g.
    /// `upstream.target`, `downstream.time` (Examples 7/8).
    LoadProblemField {
        /// Problem binding.
        binding: BindingIdx,
        /// Catalog field within the problem schema.
        field: FieldIdx,
        /// Destination slot.
        dst: SlotIdx,
    },
    /// `LOAD_CONST imm -> slot`.
    LoadConst {
        /// Immediate `i64` literal.
        imm: i64,
        /// Destination slot.
        dst: SlotIdx,
    },
    /// `LOAD_DURATION imm_ms -> slot` — duration literals like `500ms`, `1s`.
    LoadDuration {
        /// Duration literal.
        dur: DurationMs,
        /// Destination slot.
        dst: SlotIdx,
    },
    /// `LOAD_SCOPE_KEY -> slot` — the partition scope key (e.g. `rtx.target`
    /// when it is the scope key itself).
    LoadScopeKey {
        /// Destination slot.
        dst: SlotIdx,
    },

    // ── EXPR group (06 §4; i64 checked) ─────────────────────────────────
    /// `ADD lhs, rhs -> dst` (checked).
    Add {
        /// Left operand slot.
        lhs: SlotIdx,
        /// Right operand slot.
        rhs: SlotIdx,
        /// Destination slot.
        dst: SlotIdx,
    },
    /// `SUB lhs, rhs -> dst` (checked).
    Sub {
        /// Left operand slot.
        lhs: SlotIdx,
        /// Right operand slot.
        rhs: SlotIdx,
        /// Destination slot.
        dst: SlotIdx,
    },
    /// `MUL lhs, rhs -> dst` (checked).
    Mul {
        /// Left operand slot.
        lhs: SlotIdx,
        /// Right operand slot.
        rhs: SlotIdx,
        /// Destination slot.
        dst: SlotIdx,
    },
    /// `DIV lhs, rhs -> dst` (checked; division by zero is a
    /// `CorrelateError`, not a panic).
    Div {
        /// Left operand slot.
        lhs: SlotIdx,
        /// Right operand slot.
        rhs: SlotIdx,
        /// Destination slot.
        dst: SlotIdx,
    },
    /// `MOD lhs, rhs -> dst` (checked).
    Mod {
        /// Left operand slot.
        lhs: SlotIdx,
        /// Right operand slot.
        rhs: SlotIdx,
        /// Destination slot.
        dst: SlotIdx,
    },
    /// `CMP_EQ lhs, rhs -> dst`.
    CmpEq {
        /// Left operand slot.
        lhs: SlotIdx,
        /// Right operand slot.
        rhs: SlotIdx,
        /// Destination slot.
        dst: SlotIdx,
    },
    /// `CMP_NE lhs, rhs -> dst`.
    CmpNe {
        /// Left operand slot.
        lhs: SlotIdx,
        /// Right operand slot.
        rhs: SlotIdx,
        /// Destination slot.
        dst: SlotIdx,
    },
    /// `CMP_LT lhs, rhs -> dst`.
    CmpLt {
        /// Left operand slot.
        lhs: SlotIdx,
        /// Right operand slot.
        rhs: SlotIdx,
        /// Destination slot.
        dst: SlotIdx,
    },
    /// `CMP_LE lhs, rhs -> dst`.
    CmpLe {
        /// Left operand slot.
        lhs: SlotIdx,
        /// Right operand slot.
        rhs: SlotIdx,
        /// Destination slot.
        dst: SlotIdx,
    },
    /// `CMP_GT lhs, rhs -> dst` — e.g. `rtx.segment_size > 1400`.
    CmpGt {
        /// Left operand slot.
        lhs: SlotIdx,
        /// Right operand slot.
        rhs: SlotIdx,
        /// Destination slot.
        dst: SlotIdx,
    },
    /// `CMP_GE lhs, rhs -> dst` — e.g. `c.confidence >= 80`.
    CmpGe {
        /// Left operand slot.
        lhs: SlotIdx,
        /// Right operand slot.
        rhs: SlotIdx,
        /// Destination slot.
        dst: SlotIdx,
    },
    /// `AND lhs, rhs -> dst` — Kleene over `T3` (`03` §3.7; evaluator
    /// short-circuits: RHS not evaluated when LHS ∈ {False, Unknown}).
    And {
        /// Left operand slot.
        lhs: SlotIdx,
        /// Right operand slot.
        rhs: SlotIdx,
        /// Destination slot.
        dst: SlotIdx,
    },
    /// `OR lhs, rhs -> dst` — Kleene over `T3`.
    Or {
        /// Left operand slot.
        lhs: SlotIdx,
        /// Right operand slot.
        rhs: SlotIdx,
        /// Destination slot.
        dst: SlotIdx,
    },
    /// `NOT src -> dst` — Kleene negation.
    Not {
        /// Source slot.
        src: SlotIdx,
        /// Destination slot.
        dst: SlotIdx,
    },

    // ── Binding presence primaries (03 §3.7, 06 §3.1 BranchTable cond) ──
    /// `present(x)` → `T3` from the correlate binding state
    /// (`03-semantics.md` §3.2: Some → True, Absent → False, Unknown →
    /// Unknown).
    Present {
        /// Correlate binding tested for presence.
        binding: BindingIdx,
        /// Destination slot.
        dst: SlotIdx,
    },
    /// `absent(x)` → `T3` (Absent → True, Some → False, Unknown → Unknown).
    Absent {
        /// Correlate binding tested for absence.
        binding: BindingIdx,
        /// Destination slot.
        dst: SlotIdx,
    },

    // ── TOPO group (06 §4) ──────────────────────────────────────────────
    /// `TOPO_CALL func_idx, slot_a, slot_b -> slot_t3` — topology functions
    /// return `T3` (C10; `07-runtime.md` §6).
    TopoCall {
        /// Catalog topology function.
        func: TopoFuncIdx,
        /// First scope-key argument slot.
        a: SlotIdx,
        /// Second scope-key argument slot.
        b: SlotIdx,
        /// Destination slot (holds a `T3`).
        dst: SlotIdx,
    },

    // ── WIN group (06 §4) ───────────────────────────────────────────────
    /// `WIN_BACK anchor.time, dur -> slot` — lower window bound
    /// `anchor.time - dur`.
    WinBack {
        /// Slot holding the anchor time.
        time: SlotIdx,
        /// Slot holding the backward duration.
        dur: SlotIdx,
        /// Destination slot.
        dst: SlotIdx,
    },
    /// `WIN_FWD anchor.time, dur -> slot` — upper window bound
    /// `anchor.time + dur`.
    WinFwd {
        /// Slot holding the anchor time.
        time: SlotIdx,
        /// Slot holding the forward duration.
        dur: SlotIdx,
        /// Destination slot.
        dst: SlotIdx,
    },
    /// `WIN_IN x.time, lo, hi -> slot_bool` — inclusive both ends (D4;
    /// `05-verification.md` §3.2).
    WinIn {
        /// Slot holding the candidate time.
        x: SlotIdx,
        /// Slot holding the inclusive lower bound.
        lo: SlotIdx,
        /// Slot holding the inclusive upper bound.
        hi: SlotIdx,
        /// Destination slot.
        dst: SlotIdx,
    },
}

/// A compiled predicate: a straight-line opcode sequence whose value is read
/// from `result` after the last op (`06-ir-bytecode.md` §4; no control flow
/// inside a predicate — ADGL control flow lives in [`crate::BranchTable`] and
/// the WaitQueue, `06` §3).
///
/// Anchor predicates are `Bool`-valued (`03` §3.1 "if ⟦p⟧(Γ) == true");
/// branch conditions are `T3`-valued (`03` §3.7).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Predicate {
    /// Ordered opcode sequence.
    pub ops: Box<[PredOp]>,
    /// Slot holding the predicate value after executing `ops`.
    pub result: SlotIdx,
}

impl Predicate {
    /// A predicate that is trivially true — used for anchors declared without
    /// a metric predicate block, e.g. Example 8
    /// `anchor downstream: Problem(DeviceUnreachable)`.
    #[must_use]
    pub fn always_true() -> Predicate {
        let s0 = SlotIdx(0);
        Predicate {
            ops: Box::new([PredOp::LoadConst { imm: 1, dst: s0 }]),
            result: s0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slot_idx_is_bounded_by_max_slots() {
        assert!(SlotIdx::new(0).is_some());
        let last = SlotIdx::new((MAX_SLOTS - 1) as u8).expect("last slot is valid");
        assert_eq!(last.index(), MAX_SLOTS - 1);
        assert!(SlotIdx::new(MAX_SLOTS as u8).is_none());
        assert!(SlotIdx::new(u8::MAX).is_none());
    }

    #[test]
    fn always_true_shape() {
        let p = Predicate::always_true();
        assert_eq!(p.ops.len(), 1);
        assert!(matches!(p.ops[0], PredOp::LoadConst { imm: 1, dst } if dst == p.result));
    }
}
