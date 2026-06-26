//! Minimal Session DB (EFSM)

use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FlowKey {
    pub data: [u8; 16], // simplified 4-tuple hash
}

#[derive(Debug, Clone)]
pub struct SessionContext {
    pub state: u32,
    pub vars: HashMap<String, u64>,
}

#[derive(Debug, Clone)]
pub struct SessionDb {
    pub sessions: HashMap<FlowKey, SessionContext>,
    pub max_sessions: usize,
}

impl SessionDb {
    pub fn new(max_sessions: usize) -> Self {
        Self {
            sessions: HashMap::new(),
            max_sessions,
        }
    }

    pub fn get_or_create(&mut self, key: FlowKey) -> &mut SessionContext {
        if self.sessions.len() >= self.max_sessions && !self.sessions.contains_key(&key) {
            // LRU eviction would happen here in real impl
        }

        self.sessions.entry(key).or_insert(SessionContext {
            state: 0,
            vars: HashMap::new(),
        })
    }

    pub fn transition(&mut self, key: &FlowKey, new_state: u32, var: Option<(String, u64)>) {
        if let Some(sess) = self.sessions.get_mut(key) {
            sess.state = new_state;
            if let Some((k, v)) = var {
                sess.vars.insert(k, v);
            }
        }
    }

    pub fn len(&self) -> usize {
        self.sessions.len()
    }
}
