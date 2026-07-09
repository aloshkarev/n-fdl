//! Predicate interpreter per `docs/idea/spec/06-ir-bytecode.md` §4:
//! straight-line opcode sequences over a fixed, stack-allocated slot-register
//! array (`07-runtime.md` §8 — no heap in the hot path).
//!
//! Semantics:
//! - `i64` arithmetic is *checked* — overflow/division-by-zero surface as
//!   [`CorrelateError`] values, never panics (`06` §8 item 6, `07` §9);
//! - comparisons are `Bool` lifted to [`T3`]; `AND`/`OR`/`NOT` are Kleene
//!   (`03-semantics.md` §3.7);
//! - loads from an `Absent`/`Unknown` binding, and loads of a field absent
//!   on the bound value, produce `Unknown` — this is the straight-line
//!   realization of the mandatory short-circuit rule (`03` §3.7): the
//!   guarded RHS of `present(x) and x.f == v` evaluates to `Unknown`, and
//!   Kleene `False and Unknown = False` absorbs it exactly as if the RHS
//!   had not been evaluated;
//! - `Unknown` propagates through arithmetic/comparison operands (C10).

use airpulse_dsl_ir::{BindingIdx, FieldIdx, MAX_SLOTS, PredOp, Predicate, SlotIdx};
use airpulse_dsl_types::{ScopeId, T3};

use crate::binding::{Binding, Bound};
use crate::error::CorrelateError;
use crate::interner::{ScopeInterner, scope_key_i64};
use crate::schema::{
    CAUSE_FIELD_CONFIDENCE, CAUSE_FIELD_TARGET, CAUSE_FIELD_TIME, EVENT_FIELD_TARGET,
    EVENT_FIELD_TIME, PROBLEM_FIELD_TARGET, PROBLEM_FIELD_TIME,
};
use crate::topology::{TopoFunc, TopologyProvider};

/// Evaluation context: resolved bindings (anchor = index 0, correlates
/// follow in declaration order — `06` §2.1), the partition scope, the
/// scope-key intern table and the topology oracle.
pub struct PredCtx<'a> {
    /// Binding states, anchor first.
    pub bindings: &'a [Binding],
    /// Partition scope (`LOAD_SCOPE_KEY`).
    pub scope: ScopeId,
    /// Scope-key intern table (topology argument resolution).
    pub interner: &'a ScopeInterner,
    /// Topology oracle for `TOPO_CALL`.
    pub topo: &'a dyn TopologyProvider,
}

/// One slot register value. `Truth(Unknown)` doubles as the
/// "missing/unbound" marker (see module docs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Slot {
    /// Never written.
    Empty,
    /// An `i64` value (metric field, const, duration, interned scope key,
    /// time in ms).
    Int(i64),
    /// A three-valued logic value.
    Truth(T3),
}

/// Numeric operand view: `Ok(Some(i))` for ints, `Ok(None)` when the operand
/// is `Unknown` (propagates), `Err` for definite type errors.
fn as_num(slot: Slot, op: &'static str) -> Result<Option<i64>, CorrelateError> {
    match slot {
        Slot::Int(i) => Ok(Some(i)),
        Slot::Truth(T3::Unknown) => Ok(None),
        Slot::Truth(_) | Slot::Empty => Err(CorrelateError::TypeMismatch { op }),
    }
}

/// Truth operand view: `T3` values pass through, ints lift (`0 = False`,
/// non-zero = `True` — `03` §3.7 Bool lifting).
fn as_truth(slot: Slot, op: &'static str) -> Result<T3, CorrelateError> {
    match slot {
        Slot::Truth(t) => Ok(t),
        Slot::Int(i) => Ok(T3::from(i != 0)),
        Slot::Empty => Err(CorrelateError::TypeMismatch { op }),
    }
}

fn binding<'a>(
    bindings: &'a [Binding],
    idx: BindingIdx,
) -> Result<&'a Binding, CorrelateError> {
    bindings.get(idx.0 as usize).ok_or(CorrelateError::UnknownBinding { binding: idx.0 })
}

fn load_event_field(b: &Binding, field: FieldIdx) -> Result<Slot, CorrelateError> {
    match b {
        Binding::Bound(Bound::Event(e)) => Ok(match field {
            f if f == EVENT_FIELD_TIME => Slot::Int(e.time.millis()),
            f if f == EVENT_FIELD_TARGET => {
                Slot::Int(e.field(EVENT_FIELD_TARGET).unwrap_or_else(|| scope_key_i64(e.scope)))
            }
            f => match e.field(f) {
                Some(v) => Slot::Int(v),
                None => Slot::Truth(T3::Unknown), // Option<τ> field absent (04 §1)
            },
        }),
        Binding::Bound(_) => Err(CorrelateError::TypeMismatch { op: "LOAD_EVENT_FIELD" }),
        Binding::Absent | Binding::Unknown => Ok(Slot::Truth(T3::Unknown)),
    }
}

fn load_cause_field(b: &Binding, field: FieldIdx) -> Result<Slot, CorrelateError> {
    match b {
        Binding::Bound(Bound::Cause(c)) => match field {
            f if f == CAUSE_FIELD_CONFIDENCE => Ok(Slot::Int(i64::from(c.confidence.value()))),
            f if f == CAUSE_FIELD_TARGET => Ok(Slot::Int(scope_key_i64(c.target))),
            f if f == CAUSE_FIELD_TIME => Ok(Slot::Int(c.time.millis())),
            _ => Ok(Slot::Truth(T3::Unknown)),
        },
        Binding::Bound(_) => Err(CorrelateError::TypeMismatch { op: "LOAD_CAUSE_FIELD" }),
        Binding::Absent | Binding::Unknown => Ok(Slot::Truth(T3::Unknown)),
    }
}

fn load_problem_field(b: &Binding, field: FieldIdx) -> Result<Slot, CorrelateError> {
    match b {
        Binding::Bound(Bound::Problem(p)) => match field {
            f if f == PROBLEM_FIELD_TARGET => Ok(Slot::Int(scope_key_i64(p.target))),
            f if f == PROBLEM_FIELD_TIME => Ok(Slot::Int(p.time.millis())),
            _ => Ok(Slot::Truth(T3::Unknown)),
        },
        Binding::Bound(_) => Err(CorrelateError::TypeMismatch { op: "LOAD_PROBLEM_FIELD" }),
        Binding::Absent | Binding::Unknown => Ok(Slot::Truth(T3::Unknown)),
    }
}

/// Checked binary arithmetic with `Unknown` propagation.
fn arith(
    lhs: Slot,
    rhs: Slot,
    op: &'static str,
    f: impl Fn(i64, i64) -> Result<Option<i64>, CorrelateError>,
) -> Result<Slot, CorrelateError> {
    match (as_num(lhs, op)?, as_num(rhs, op)?) {
        (Some(a), Some(b)) => Ok(match f(a, b)? {
            Some(v) => Slot::Int(v),
            None => Slot::Truth(T3::Unknown),
        }),
        _ => Ok(Slot::Truth(T3::Unknown)),
    }
}

/// Comparison lifted to `T3`, with `Unknown` propagation.
fn cmp(
    lhs: Slot,
    rhs: Slot,
    op: &'static str,
    f: impl Fn(i64, i64) -> bool,
) -> Result<Slot, CorrelateError> {
    match (as_num(lhs, op)?, as_num(rhs, op)?) {
        (Some(a), Some(b)) => Ok(Slot::Truth(T3::from(f(a, b)))),
        _ => Ok(Slot::Truth(T3::Unknown)),
    }
}

/// Executes a compiled predicate against the context, returning the `T3`
/// value of the result slot. Anchor predicates require `True` to match
/// (`03` §3.1); branch conditions dispatch on all three values (`03` §3.7).
pub fn eval_predicate(pred: &Predicate, ctx: &PredCtx<'_>) -> Result<T3, CorrelateError> {
    let mut slots = [Slot::Empty; MAX_SLOTS];
    let get = |slots: &[Slot; MAX_SLOTS], idx: SlotIdx| slots[idx.index()];
    let checked = |r: Option<i64>| r.map(Some).ok_or(CorrelateError::ArithOverflow);

    for op in &pred.ops {
        match op {
            PredOp::LoadEventField { binding: b, field, dst } => {
                slots[dst.index()] = load_event_field(binding(ctx.bindings, *b)?, *field)?;
            }
            PredOp::LoadCauseField { binding: b, field, dst } => {
                slots[dst.index()] = load_cause_field(binding(ctx.bindings, *b)?, *field)?;
            }
            PredOp::LoadProblemField { binding: b, field, dst } => {
                slots[dst.index()] = load_problem_field(binding(ctx.bindings, *b)?, *field)?;
            }
            PredOp::LoadConst { imm, dst } => slots[dst.index()] = Slot::Int(*imm),
            PredOp::LoadDuration { dur, dst } => slots[dst.index()] = Slot::Int(dur.millis()),
            PredOp::LoadScopeKey { dst } => {
                slots[dst.index()] = Slot::Int(scope_key_i64(ctx.scope));
            }

            PredOp::Add { lhs, rhs, dst } => {
                slots[dst.index()] =
                    arith(get(&slots, *lhs), get(&slots, *rhs), "ADD", |a, b| {
                        checked(a.checked_add(b))
                    })?;
            }
            PredOp::Sub { lhs, rhs, dst } => {
                slots[dst.index()] =
                    arith(get(&slots, *lhs), get(&slots, *rhs), "SUB", |a, b| {
                        checked(a.checked_sub(b))
                    })?;
            }
            PredOp::Mul { lhs, rhs, dst } => {
                slots[dst.index()] =
                    arith(get(&slots, *lhs), get(&slots, *rhs), "MUL", |a, b| {
                        checked(a.checked_mul(b))
                    })?;
            }
            PredOp::Div { lhs, rhs, dst } => {
                slots[dst.index()] =
                    arith(get(&slots, *lhs), get(&slots, *rhs), "DIV", |a, b| {
                        if b == 0 {
                            Err(CorrelateError::DivisionByZero)
                        } else {
                            checked(a.checked_div(b))
                        }
                    })?;
            }
            PredOp::Mod { lhs, rhs, dst } => {
                slots[dst.index()] =
                    arith(get(&slots, *lhs), get(&slots, *rhs), "MOD", |a, b| {
                        if b == 0 {
                            Err(CorrelateError::DivisionByZero)
                        } else {
                            checked(a.checked_rem(b))
                        }
                    })?;
            }

            PredOp::CmpEq { lhs, rhs, dst } => {
                slots[dst.index()] = cmp(get(&slots, *lhs), get(&slots, *rhs), "CMP_EQ", |a, b| a == b)?;
            }
            PredOp::CmpNe { lhs, rhs, dst } => {
                slots[dst.index()] = cmp(get(&slots, *lhs), get(&slots, *rhs), "CMP_NE", |a, b| a != b)?;
            }
            PredOp::CmpLt { lhs, rhs, dst } => {
                slots[dst.index()] = cmp(get(&slots, *lhs), get(&slots, *rhs), "CMP_LT", |a, b| a < b)?;
            }
            PredOp::CmpLe { lhs, rhs, dst } => {
                slots[dst.index()] = cmp(get(&slots, *lhs), get(&slots, *rhs), "CMP_LE", |a, b| a <= b)?;
            }
            PredOp::CmpGt { lhs, rhs, dst } => {
                slots[dst.index()] = cmp(get(&slots, *lhs), get(&slots, *rhs), "CMP_GT", |a, b| a > b)?;
            }
            PredOp::CmpGe { lhs, rhs, dst } => {
                slots[dst.index()] = cmp(get(&slots, *lhs), get(&slots, *rhs), "CMP_GE", |a, b| a >= b)?;
            }

            PredOp::And { lhs, rhs, dst } => {
                let a = as_truth(get(&slots, *lhs), "AND")?;
                let b = as_truth(get(&slots, *rhs), "AND")?;
                slots[dst.index()] = Slot::Truth(a.and(b));
            }
            PredOp::Or { lhs, rhs, dst } => {
                let a = as_truth(get(&slots, *lhs), "OR")?;
                let b = as_truth(get(&slots, *rhs), "OR")?;
                slots[dst.index()] = Slot::Truth(a.or(b));
            }
            PredOp::Not { src, dst } => {
                let t = as_truth(get(&slots, *src), "NOT")?;
                slots[dst.index()] = Slot::Truth(t.not());
            }

            PredOp::Present { binding: b, dst } => {
                let t = match binding(ctx.bindings, *b)? {
                    Binding::Bound(_) => T3::True,
                    Binding::Absent => T3::False,
                    Binding::Unknown => T3::Unknown,
                };
                slots[dst.index()] = Slot::Truth(t);
            }
            PredOp::Absent { binding: b, dst } => {
                let t = match binding(ctx.bindings, *b)? {
                    Binding::Absent => T3::True,
                    Binding::Bound(_) => T3::False,
                    Binding::Unknown => T3::Unknown,
                };
                slots[dst.index()] = Slot::Truth(t);
            }

            PredOp::TopoCall { func, a, b, dst } => {
                let f = TopoFunc::from_idx(*func)
                    .ok_or(CorrelateError::UnknownTopoFunction { func: func.0 })?;
                let t = match (as_num(get(&slots, *a), "TOPO_CALL")?, as_num(get(&slots, *b), "TOPO_CALL")?)
                {
                    (Some(ka), Some(kb)) => {
                        match (ctx.interner.resolve(ka), ctx.interner.resolve(kb)) {
                            (Some(sa), Some(sb)) => f.call(ctx.topo, sa, sb),
                            // Un-interned key: topology identity is unknowable.
                            _ => T3::Unknown,
                        }
                    }
                    _ => T3::Unknown,
                };
                slots[dst.index()] = Slot::Truth(t);
            }

            PredOp::WinBack { time, dur, dst } => {
                slots[dst.index()] =
                    arith(get(&slots, *time), get(&slots, *dur), "WIN_BACK", |t, d| {
                        checked(t.checked_sub(d))
                    })?;
            }
            PredOp::WinFwd { time, dur, dst } => {
                slots[dst.index()] =
                    arith(get(&slots, *time), get(&slots, *dur), "WIN_FWD", |t, d| {
                        checked(t.checked_add(d))
                    })?;
            }
            PredOp::WinIn { x, lo, hi, dst } => {
                let vals = (
                    as_num(get(&slots, *x), "WIN_IN")?,
                    as_num(get(&slots, *lo), "WIN_IN")?,
                    as_num(get(&slots, *hi), "WIN_IN")?,
                );
                slots[dst.index()] = Slot::Truth(match vals {
                    // Inclusive both ends (D4, 06 §4).
                    (Some(x), Some(lo), Some(hi)) => T3::from(lo <= x && x <= hi),
                    _ => T3::Unknown,
                });
            }
        }
    }
    as_truth(slots[pred.result.index()], "RESULT")
}

#[cfg(test)]
mod tests {
    use airpulse_dsl_store::{EventNode, EventProvenance};
    use airpulse_dsl_types::{EventId, EventTime, EventType};

    use super::*;
    use crate::topology::StaticTopology;

    fn slot(i: u8) -> SlotIdx {
        SlotIdx::new(i).expect("test slot within MAX_SLOTS")
    }

    fn event(fields: Vec<(FieldIdx, i64)>) -> EventNode {
        EventNode::new(
            EventId::new(1),
            EventType::new("tcp.retransmission_burst"),
            EventTime::from_millis(10_000),
            ScopeId::vlan(1),
            fields,
            EventProvenance::default(),
        )
    }

    fn run_pred(pred: &Predicate, bindings: &[Binding]) -> Result<T3, CorrelateError> {
        let topo = StaticTopology::new(4);
        let mut interner = ScopeInterner::new();
        interner.intern(ScopeId::vlan(1));
        let ctx = PredCtx { bindings, scope: ScopeId::vlan(1), interner: &interner, topo: &topo };
        eval_predicate(pred, &ctx)
    }

    #[test]
    fn checked_arithmetic_yields_error_values_not_panics() {
        // 06 §8 item 6: overflow → CorrelateError::ArithOverflow.
        let overflow = Predicate {
            ops: Box::new([
                PredOp::LoadConst { imm: i64::MAX, dst: slot(0) },
                PredOp::LoadConst { imm: 1, dst: slot(1) },
                PredOp::Add { lhs: slot(0), rhs: slot(1), dst: slot(2) },
            ]),
            result: slot(2),
        };
        assert_eq!(run_pred(&overflow, &[]), Err(CorrelateError::ArithOverflow));

        let div_zero = Predicate {
            ops: Box::new([
                PredOp::LoadConst { imm: 7, dst: slot(0) },
                PredOp::LoadConst { imm: 0, dst: slot(1) },
                PredOp::Div { lhs: slot(0), rhs: slot(1), dst: slot(2) },
            ]),
            result: slot(2),
        };
        assert_eq!(run_pred(&div_zero, &[]), Err(CorrelateError::DivisionByZero));

        // i64::MIN / -1 overflows too.
        let min_div = Predicate {
            ops: Box::new([
                PredOp::LoadConst { imm: i64::MIN, dst: slot(0) },
                PredOp::LoadConst { imm: -1, dst: slot(1) },
                PredOp::Div { lhs: slot(0), rhs: slot(1), dst: slot(2) },
            ]),
            result: slot(2),
        };
        assert_eq!(run_pred(&min_div, &[]), Err(CorrelateError::ArithOverflow));
    }

    #[test]
    fn metric_comparison_lifts_to_t3() {
        // 03 §3.7: metric comparisons are Bool lifted to T3.
        let gt_1400 = Predicate {
            ops: Box::new([
                PredOp::LoadEventField { binding: BindingIdx(0), field: FieldIdx(0), dst: slot(0) },
                PredOp::LoadConst { imm: 1400, dst: slot(1) },
                PredOp::CmpGt { lhs: slot(0), rhs: slot(1), dst: slot(2) },
            ]),
            result: slot(2),
        };
        let big = [Binding::Bound(Bound::Event(event(vec![(FieldIdx(0), 1500)])))];
        let small = [Binding::Bound(Bound::Event(event(vec![(FieldIdx(0), 1200)])))];
        assert_eq!(run_pred(&gt_1400, &big), Ok(T3::True));
        assert_eq!(run_pred(&gt_1400, &small), Ok(T3::False));
        // Missing optional field (04 §1) → Unknown, not an error.
        let missing = [Binding::Bound(Bound::Event(event(vec![])))];
        assert_eq!(run_pred(&gt_1400, &missing), Ok(T3::Unknown));
    }

    #[test]
    fn kleene_absorbs_guarded_unbound_access() {
        // 03 §3.7 short-circuit pattern `present(x) and x.f == v`: with x
        // Absent, the RHS load yields Unknown and Kleene False∧Unknown =
        // False — equivalent to never evaluating the RHS.
        let guarded = Predicate {
            ops: Box::new([
                PredOp::Present { binding: BindingIdx(0), dst: slot(0) },
                PredOp::LoadEventField { binding: BindingIdx(0), field: FieldIdx(0), dst: slot(1) },
                PredOp::LoadConst { imm: 1, dst: slot(2) },
                PredOp::CmpEq { lhs: slot(1), rhs: slot(2), dst: slot(3) },
                PredOp::And { lhs: slot(0), rhs: slot(3), dst: slot(4) },
            ]),
            result: slot(4),
        };
        assert_eq!(run_pred(&guarded, &[Binding::Absent]), Ok(T3::False));
        assert_eq!(run_pred(&guarded, &[Binding::Unknown]), Ok(T3::Unknown));
        let bound = [Binding::Bound(Bound::Event(event(vec![(FieldIdx(0), 1)])))];
        assert_eq!(run_pred(&guarded, &bound), Ok(T3::True));
    }

    #[test]
    fn present_absent_are_three_valued() {
        // 03 §3.2: present/absent over the binding state; Unknown is
        // neither (C10).
        let present = Predicate {
            ops: Box::new([PredOp::Present { binding: BindingIdx(0), dst: slot(0) }]),
            result: slot(0),
        };
        let absent = Predicate {
            ops: Box::new([PredOp::Absent { binding: BindingIdx(0), dst: slot(0) }]),
            result: slot(0),
        };
        let bound = [Binding::Bound(Bound::Event(event(vec![])))];
        assert_eq!(run_pred(&present, &bound), Ok(T3::True));
        assert_eq!(run_pred(&absent, &bound), Ok(T3::False));
        assert_eq!(run_pred(&present, &[Binding::Absent]), Ok(T3::False));
        assert_eq!(run_pred(&absent, &[Binding::Absent]), Ok(T3::True));
        assert_eq!(run_pred(&present, &[Binding::Unknown]), Ok(T3::Unknown));
        assert_eq!(run_pred(&absent, &[Binding::Unknown]), Ok(T3::Unknown));
        // Out-of-range binding index is an error value, not a panic.
        assert_eq!(run_pred(&present, &[]), Err(CorrelateError::UnknownBinding { binding: 0 }));
    }

    #[test]
    fn win_in_is_inclusive_both_ends() {
        // D4 / 06 §4: WIN_IN inclusive at both boundaries.
        let win = |x: i64| Predicate {
            ops: Box::new([
                PredOp::LoadConst { imm: x, dst: slot(0) },
                PredOp::LoadConst { imm: 100, dst: slot(1) },
                PredOp::LoadConst { imm: 200, dst: slot(2) },
                PredOp::WinIn { x: slot(0), lo: slot(1), hi: slot(2), dst: slot(3) },
            ]),
            result: slot(3),
        };
        assert_eq!(run_pred(&win(100), &[]), Ok(T3::True));
        assert_eq!(run_pred(&win(200), &[]), Ok(T3::True));
        assert_eq!(run_pred(&win(99), &[]), Ok(T3::False));
        assert_eq!(run_pred(&win(201), &[]), Ok(T3::False));
    }
}
