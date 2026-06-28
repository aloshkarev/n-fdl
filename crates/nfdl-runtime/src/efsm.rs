//! Advanced EFSM for production v1: real guards, set/emit actions, per-flow state names.

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
        // A fresh flow has no recorded current state. Per machine, the initial
        // state (e.g. TCP `CLOSED`, not the generic `IDLE`) is where transitions
        // are declared, so seed from the machine's `initial` before lookup.
        let mut current = self.get_current_state(&key);
        let mut events = vec![];
        let mut new_state = current.clone();

        // Look in all machines
        for (machine_name, states) in &self.machines {
            if current == "IDLE" {
                if let Some(init) = self.initials.get(machine_name) {
                    current = init.clone();
                    new_state = current.clone();
                }
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
}
