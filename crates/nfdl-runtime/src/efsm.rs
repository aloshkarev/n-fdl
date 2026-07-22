//! Advanced EFSM for production v1: real guards, set/emit actions, per-flow state names.
//!
//! Timers (v1.5 stub): a per-`FlowKey` table of named deadlines on a logical clock.
//! Firing feeds a pseudo-message `timer:<name>` through the normal transition path.
//! Idle eviction reuses `SessionDb::expire_older_than` and clears engine maps.

use crate::event_bus::Event;
use crate::session::{FlowKey, SessionDb};
use nfdl_syntax::ast::{Action, Expr, StateMachine as AstSm, Transition as AstTrans};
use std::collections::HashMap;

/// Map a state name to a numeric id for `FsmTransition`/`SessionDb`.
fn state_numeric(s: &str) -> u32 {
    match s {
        "IDLE" | "CLOSED" => 0,
        "PENDING" | "SYN_SENT" | "ESTABLISHED" => 1,
        "FIN_WAIT" => 2,
        _ => 0,
    }
}

fn flow_key_hash(key: &FlowKey) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in key.data {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x1000_0000_01b3);
    }
    h
}

/// Result of a due timer that was fired through the transition engine.
#[derive(Debug, Clone)]
pub struct FiredTimer {
    pub key: FlowKey,
    pub timer: String,
    pub new_state: String,
    pub events: Vec<Event>,
}

#[derive(Debug, Clone)]
struct ArmedTimer {
    deadline: u64,
    insertion_id: u64,
}

#[derive(Debug, Clone)]
pub struct FsmEngine {
    pub db: SessionDb,
    // machine name -> (state_name -> list of transitions)
    machines: HashMap<String, HashMap<String, Vec<AstTrans>>>,
    // machine name -> key expr (for computing FlowKey)
    keys: HashMap<String, Option<Expr>>,
    // machine name -> initial state name (a fresh flow starts here, not "IDLE")
    initials: HashMap<String, String>,
    current_states: HashMap<FlowKey, String>, // per-flow current state name
    variables: HashMap<FlowKey, HashMap<String, u64>>, // per-flow variables from set
    /// Per-flow named timers (minimal table; not a hierarchical wheel).
    timers: HashMap<FlowKey, HashMap<String, ArmedTimer>>,
    /// Logical clock for timer deadlines (independent of session LRU ticks).
    timer_now: u64,
    timer_seq: u64,
}

impl FsmEngine {
    pub fn new(max_sessions: usize) -> Self {
        Self {
            db: SessionDb::new(max_sessions),
            machines: HashMap::new(),
            keys: HashMap::new(),
            initials: HashMap::new(),
            current_states: HashMap::new(),
            variables: HashMap::new(),
            timers: HashMap::new(),
            timer_now: 0,
            timer_seq: 0,
        }
    }

    pub fn load_from_ast(&mut self, sms: &[AstSm]) {
        self.machines.clear();
        self.keys.clear();
        self.initials.clear();
        for sm in sms {
            let mut state_map: HashMap<String, Vec<AstTrans>> = HashMap::new();
            for st in &sm.states {
                state_map.insert(st.name.clone(), st.transitions.clone());
            }
            self.machines.insert(sm.name.clone(), state_map);
            self.keys.insert(sm.name.clone(), sm.key.clone());
            self.initials.insert(sm.name.clone(), sm.initial.clone());
        }
    }

    fn get_current_state(&self, key: &FlowKey) -> String {
        self.current_states
            .get(key)
            .cloned()
            .unwrap_or_else(|| "IDLE".to_string())
    }

    fn set_current_state(&mut self, key: &FlowKey, state: String) {
        self.current_states.insert(key.clone(), state);
    }

    fn eval_expr(&self, e: &Expr, ctx: &HashMap<String, u64>, key: &FlowKey) -> u64 {
        // Per-flow variables (from `set` actions) take priority over parsed-field context.
        let mut vars = ctx.clone();
        if let Some(flow_vars) = self.variables.get(key) {
            for (k, v) in flow_vars {
                vars.insert(k.clone(), *v);
            }
        }
        // Delegate to the full operator evaluator (covers all BinOp/Unary/Ternary/Coalesce).
        crate::integration::eval_expr(e, &vars)
    }

    fn eval_guard(&self, guard: &Option<Expr>, ctx: &HashMap<String, u64>, key: &FlowKey) -> bool {
        match guard {
            None => true,
            Some(e) => self.eval_expr(e, ctx, key) != 0,
        }
    }

    /// Main feed: uses full parsed transitions, executes actions, returns events
    pub fn feed(
        &mut self,
        key: FlowKey,
        msg_type: &str,
        ctx: &HashMap<String, u64>,
    ) -> (String, Vec<Event>) {
        let (_, evicted) = self.db.get_or_create(key.clone());
        if let Some(evicted_key) = evicted {
            self.drop_flow_maps(&evicted_key);
        }

        // A fresh flow has no recorded current state. Per machine, the initial
        // state (e.g. TCP `CLOSED`, not the generic `IDLE`) is where transitions
        // are declared, so seed from the machine's `initial` before lookup.
        let mut current = self.get_current_state(&key);
        let mut events = vec![];
        let mut new_state = current.clone();

        // Look in all machines
        for (machine_name, states) in &self.machines {
            if current == "IDLE"
                && let Some(init) = self.initials.get(machine_name)
            {
                current = init.clone();
                new_state = current.clone();
            }
            if let Some(transitions) = states.get(&current) {
                for tr in transitions {
                    if tr.msg_type == msg_type && self.eval_guard(&tr.guard, ctx, &key) {
                        new_state = tr.to_state.clone();

                        for action in &tr.actions {
                            match action {
                                Action::Set { var, value } => {
                                    let val = self.eval_expr(value, ctx, &key);
                                    let vars = self.variables.entry(key.clone()).or_default();
                                    vars.insert(var.clone(), val);
                                    events.push(Event::Diagnostic {
                                        code: "SET".into(),
                                        message: format!("{}={}", var, val),
                                    });
                                }
                                Action::Emit { event } => {
                                    events.push(Event::Emit {
                                        name: event.clone(),
                                    });
                                }
                            }
                        }

                        events.push(Event::FsmTransition {
                            from: state_numeric(&current),
                            to: state_numeric(&tr.to_state),
                            machine: machine_name.clone(),
                        });

                        break;
                    }
                }
            }
        }

        self.set_current_state(&key, new_state.clone());
        let numeric = state_numeric(&new_state);
        self.db.transition(&key, numeric, None);

        (new_state, events)
    }

    pub fn get_variables(&self, key: &FlowKey) -> Option<&HashMap<String, u64>> {
        self.variables.get(key)
    }

    /// Current logical timer clock.
    pub fn timer_now(&self) -> u64 {
        self.timer_now
    }

    /// Observe the named state for a flow, if the engine still tracks it.
    pub fn current_state(&self, key: &FlowKey) -> Option<String> {
        self.current_states.get(key).cloned()
    }

    pub fn has_timer(&self, key: &FlowKey, name: &str) -> bool {
        self.timers.get(key).is_some_and(|m| m.contains_key(name))
    }

    /// Arm (or re-arm) a named timer that fires after `delay_ticks` on the timer clock.
    pub fn schedule_timer(&mut self, key: &FlowKey, name: impl Into<String>, delay_ticks: u64) {
        let name = name.into();
        self.timer_seq = self.timer_seq.saturating_add(1);
        let deadline = self.timer_now.saturating_add(delay_ticks);
        self.timers.entry(key.clone()).or_default().insert(
            name,
            ArmedTimer {
                deadline,
                insertion_id: self.timer_seq,
            },
        );
    }

    /// Cancel a named timer. Returns true if it was armed.
    pub fn cancel_timer(&mut self, key: &FlowKey, name: &str) -> bool {
        let Some(map) = self.timers.get_mut(key) else {
            return false;
        };
        let removed = map.remove(name).is_some();
        if map.is_empty() {
            self.timers.remove(key);
        }
        removed
    }

    /// Advance the timer clock by `ticks` and fire all due timers (deadline ≤ now).
    /// Order: (deadline, insertion_id). Each fire runs `feed(key, "timer:<name>", {})`.
    pub fn advance_and_fire(&mut self, ticks: u64) -> Vec<FiredTimer> {
        self.timer_now = self.timer_now.saturating_add(ticks);
        self.fire_due_timers()
    }

    /// Fire all timers whose deadline is ≤ `timer_now` without advancing the clock.
    pub fn fire_due_timers(&mut self) -> Vec<FiredTimer> {
        let now = self.timer_now;
        let mut due: Vec<(u64, u64, FlowKey, String)> = Vec::new();
        for (key, map) in &self.timers {
            for (name, armed) in map {
                if armed.deadline <= now {
                    due.push((
                        armed.deadline,
                        armed.insertion_id,
                        key.clone(),
                        name.clone(),
                    ));
                }
            }
        }
        due.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));

        let mut fired = Vec::with_capacity(due.len());
        for (_, _, key, name) in due {
            if !self.has_timer(&key, &name) {
                continue; // cancelled while earlier timers ran
            }
            self.cancel_timer(&key, &name);
            let msg = format!("timer:{name}");
            let empty = HashMap::new();
            let (new_state, events) = self.feed(key.clone(), &msg, &empty);
            fired.push(FiredTimer {
                key,
                timer: name,
                new_state,
                events,
            });
        }
        fired
    }

    /// Drop sessions idle longer than `idle_timeout_ticks` (session DB clock).
    /// Cleans `current_states` / `variables` / timers and emits `SessionExpired`.
    pub fn expire_idle(&mut self, idle_timeout_ticks: u64) -> (usize, Vec<Event>) {
        let now = self.db.tick();
        let threshold = now.saturating_sub(idle_timeout_ticks);
        let expired_keys: Vec<FlowKey> = self
            .db
            .sessions
            .iter()
            .filter(|(_, s)| s.last_access < threshold)
            .map(|(k, _)| k.clone())
            .collect();

        let n = self.db.expire_older_than(threshold);
        let mut events = Vec::with_capacity(expired_keys.len());
        for key in &expired_keys {
            self.drop_flow_maps(key);
            events.push(Event::SessionExpired {
                key_hash: flow_key_hash(key),
            });
        }
        debug_assert_eq!(n, expired_keys.len());
        (n, events)
    }

    fn drop_flow_maps(&mut self, key: &FlowKey) {
        self.current_states.remove(key);
        self.variables.remove(key);
        self.timers.remove(key);
    }
}
