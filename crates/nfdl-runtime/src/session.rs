//! Minimal Session DB (EFSM) with LRU eviction + idle expiration.
//!
//! Tracks a logical `clock` tick bumped on every access; each session records its
//! `last_access` tick. On `get_or_create`, when capacity is reached and the key is
//! absent, the least-recently-used session is evicted. `expire_older_than` drops
//! sessions idle since the given threshold (spec `09-efsm-sessions.md`).

use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FlowKey {
    pub data: [u8; 16], // simplified 4-tuple hash
}

#[derive(Debug, Clone)]
pub struct SessionContext {
    pub state: u32,
    pub vars: HashMap<String, u64>,
    /// Logical tick of the most recent access (for LRU + expiration).
    pub last_access: u64,
}

#[derive(Debug, Clone)]
pub struct SessionDb {
    pub sessions: HashMap<FlowKey, SessionContext>,
    pub max_sessions: usize,
    clock: u64,
}

impl SessionDb {
    pub fn new(max_sessions: usize) -> Self {
        Self {
            sessions: HashMap::new(),
            max_sessions,
            clock: 0,
        }
    }

    /// Current logical tick (monotonically increasing across accesses).
    pub fn tick(&self) -> u64 {
        self.clock
    }

    /// Bump the clock and return the new tick.
    fn bump(&mut self) -> u64 {
        self.clock = self.clock.saturating_add(1);
        self.clock
    }

    /// Evict the least-recently-used session (smallest `last_access`).
    fn evict_lru(&mut self) {
        if let Some((lru_key, _)) = self
            .sessions
            .iter()
            .min_by_key(|(_, s)| s.last_access)
            .map(|(k, _)| (k.clone(), ()))
        {
            self.sessions.remove(&lru_key);
        }
    }

    pub fn get_or_create(&mut self, key: FlowKey) -> &mut SessionContext {
        let now = self.bump();
        if self.sessions.len() >= self.max_sessions && !self.sessions.contains_key(&key) {
            self.evict_lru();
        }
        let sess = self.sessions.entry(key).or_insert(SessionContext {
            state: 0,
            vars: HashMap::new(),
            last_access: now,
        });
        sess.last_access = now;
        sess
    }

    pub fn transition(&mut self, key: &FlowKey, new_state: u32, var: Option<(String, u64)>) {
        let now = self.bump();
        if let Some(sess) = self.sessions.get_mut(key) {
            sess.state = new_state;
            sess.last_access = now;
            if let Some((k, v)) = var {
                sess.vars.insert(k, v);
            }
        }
    }

    /// Drop sessions whose `last_access` is strictly older than `threshold_tick`.
    /// Returns the number of expired sessions.
    pub fn expire_older_than(&mut self, threshold_tick: u64) -> usize {
        let before = self.sessions.len();
        self.sessions.retain(|_, s| s.last_access >= threshold_tick);
        before - self.sessions.len()
    }

    pub fn len(&self) -> usize {
        self.sessions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sessions.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(n: u8) -> FlowKey {
        FlowKey { data: [n; 16] }
    }

    #[test]
    fn lru_eviction_at_capacity() {
        let mut db = SessionDb::new(2);
        db.get_or_create(key(1));
        db.get_or_create(key(2));
        // Access key(1) to make key(2) the LRU.
        db.get_or_create(key(1));
        // Inserting a third, distinct key evicts the LRU (key 2).
        db.get_or_create(key(3));
        assert_eq!(db.len(), 2);
        assert!(!db.sessions.contains_key(&key(2)));
        assert!(db.sessions.contains_key(&key(1)));
        assert!(db.sessions.contains_key(&key(3)));
    }

    #[test]
    fn expire_older_than_drops_idle() {
        let mut db = SessionDb::new(16);
        db.get_or_create(key(1)); // tick 1
        db.get_or_create(key(2)); // tick 2
        let cutoff = db.tick();
        db.get_or_create(key(3)); // tick 4 (bump inside get_or_create)
        let expired = db.expire_older_than(cutoff + 1);
        assert_eq!(
            expired, 2,
            "sessions accessed at/before cutoff should expire"
        );
        assert!(db.sessions.contains_key(&key(3)));
    }

    #[test]
    fn transition_updates_last_access() {
        let mut db = SessionDb::new(4);
        db.get_or_create(key(1));
        let t0 = db.sessions.get(&key(1)).unwrap().last_access;
        db.transition(&key(1), 5, None);
        let t1 = db.sessions.get(&key(1)).unwrap().last_access;
        assert!(t1 > t0);
        assert_eq!(db.sessions.get(&key(1)).unwrap().state, 5);
    }
}
