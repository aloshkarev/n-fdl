use nfdl_runtime::event_bus::Event;
use nfdl_runtime::session::FlowKey;
use nfdl_runtime::FsmEngine;
use nfdl_syntax::ast::{Action, State, StateMachine, Transition};
use std::collections::HashMap;

fn key(n: u8) -> FlowKey {
    FlowKey { data: [n; 16] }
}

fn idle_timeout_machine() -> StateMachine {
    StateMachine {
        name: "IdleDemo".into(),
        states: vec![
            State {
                name: "ESTABLISHED".into(),
                transitions: vec![
                    Transition {
                        from_state: Some("ESTABLISHED".into()),
                        msg_type: "Data".into(),
                        guard: None,
                        to_state: "ESTABLISHED".into(),
                        actions: vec![],
                    },
                    Transition {
                        from_state: Some("ESTABLISHED".into()),
                        msg_type: "timer:idle".into(),
                        guard: None,
                        to_state: "CLOSED".into(),
                        actions: vec![Action::Emit {
                            event: "SESSION_IDLE_TIMEOUT".into(),
                        }],
                    },
                ],
            },
            State {
                name: "CLOSED".into(),
                transitions: vec![],
            },
        ],
        initial: "ESTABLISHED".into(),
        key: None,
    }
}

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

#[test]
fn timer_fire_changes_state() {
    let mut fsm = FsmEngine::new(16);
    fsm.load_from_ast(&[idle_timeout_machine()]);

    let k = key(1);
    let ctx = HashMap::new();
    let (state, _) = fsm.feed(k.clone(), "Data", &ctx);
    assert_eq!(state, "ESTABLISHED");

    fsm.schedule_timer(&k, "idle", 5);
    assert!(fsm.has_timer(&k, "idle"));

    // Not due yet.
    let fired = fsm.advance_and_fire(4);
    assert!(fired.is_empty());
    assert_eq!(fsm.current_state(&k).as_deref(), Some("ESTABLISHED"));
    assert!(fsm.has_timer(&k, "idle"));

    // Fire at deadline: timer:idle transition → CLOSED.
    let fired = fsm.advance_and_fire(1);
    assert_eq!(fired.len(), 1);
    assert_eq!(fired[0].timer, "idle");
    assert_eq!(fired[0].new_state, "CLOSED");
    assert!(
        fired[0]
            .events
            .iter()
            .any(|e| matches!(e, Event::Emit { name } if name == "SESSION_IDLE_TIMEOUT"))
    );
    assert!(!fsm.has_timer(&k, "idle"));
    assert_eq!(fsm.current_state(&k).as_deref(), Some("CLOSED"));
}

#[test]
fn timer_cancel_prevents_fire() {
    let mut fsm = FsmEngine::new(16);
    fsm.load_from_ast(&[idle_timeout_machine()]);

    let k = key(2);
    let ctx = HashMap::new();
    fsm.feed(k.clone(), "Data", &ctx);
    fsm.schedule_timer(&k, "idle", 3);
    assert!(fsm.cancel_timer(&k, "idle"));
    assert!(!fsm.has_timer(&k, "idle"));

    let fired = fsm.advance_and_fire(10);
    assert!(fired.is_empty());
    assert_eq!(fsm.current_state(&k).as_deref(), Some("ESTABLISHED"));
}

#[test]
fn idle_expire_drops_session_and_engine_state() {
    let mut fsm = FsmEngine::new(16);
    fsm.load_from_ast(&[idle_timeout_machine()]);

    let k1 = key(10);
    let k2 = key(11);
    let ctx = HashMap::new();
    fsm.feed(k1.clone(), "Data", &ctx);
    fsm.schedule_timer(&k1, "idle", 100);
    fsm.feed(k2.clone(), "Data", &ctx);

    // Advance session clock without touching k1: bump via k2 feeds.
    for _ in 0..5 {
        fsm.feed(k2.clone(), "Data", &ctx);
    }

    let (expired, events) = fsm.expire_idle(3);
    assert!(expired >= 1, "at least the idle flow should expire");
    assert!(!fsm.db.sessions.contains_key(&k1));
    assert!(fsm.db.sessions.contains_key(&k2));
    assert!(fsm.current_state(&k1).is_none());
    assert!(!fsm.has_timer(&k1, "idle"));
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::SessionExpired { .. }))
    );
}

#[test]
fn lru_eviction_still_works_via_engine() {
    let mut fsm = FsmEngine::new(2);
    fsm.load_from_ast(&[idle_timeout_machine()]);
    let ctx = HashMap::new();

    fsm.feed(key(1), "Data", &ctx);
    fsm.feed(key(2), "Data", &ctx);
    // Touch key(1) so key(2) is LRU.
    fsm.feed(key(1), "Data", &ctx);
    fsm.feed(key(3), "Data", &ctx);

    assert_eq!(fsm.db.len(), 2);
    assert!(!fsm.db.sessions.contains_key(&key(2)));
    assert!(fsm.db.sessions.contains_key(&key(1)));
    assert!(fsm.db.sessions.contains_key(&key(3)));
    // Engine maps for the LRU-evicted flow should be cleaned.
    assert!(fsm.current_state(&key(2)).is_none());
}
