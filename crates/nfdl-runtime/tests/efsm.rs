use nfdl_runtime::FsmEngine;
use nfdl_runtime::session::FlowKey;
use std::collections::HashMap;

#[test]
fn efsm_guard_eval() {
    let mut fsm = FsmEngine::new(10);
    let key = FlowKey { data: [0; 16] };
    let mut ctx = HashMap::new();
    ctx.insert("code".to_string(), 1);

    let (state, _events) = fsm.feed(key.clone(), "AccessMessage", &ctx);
    // No transitions loaded yet; engine returns a default state name.
    assert!(!state.is_empty());
}

#[test]
fn efsm_basic_transition() {
    let mut fsm = FsmEngine::new(10);
    let key = FlowKey { data: [0; 16] };
    let ctx = HashMap::new();
    let (s1, _) = fsm.feed(key.clone(), "AccessMessage", &ctx);
    let (s2, _) = fsm.feed(key, "AccessMessage", &ctx);
    assert_eq!(s1, s2);
}
