//! Scope-key interning into the `i64` field domain.
//!
//! `airpulse_dsl-store`'s `EventNode` docs: "Catalog resolution interns
//! non-integer field values (enum strings, scope keys via their deterministic
//! hash) into the `i64` slot domain". The catalog now resolves schema/field
//! indices; this interner remains the runtime reverse table for scope-key
//! lookups: harnesses intern the `ScopeId`s their synthetic events refer to,
//! events carry the interned key in reserved
//! [`crate::schema::EVENT_FIELD_TARGET`], and target resolution maps the key
//! back to the `ScopeId`.

use std::collections::HashMap;

use airpulse_dsl_types::ScopeId;

/// The deterministic `i64` intern key of a scope — a reinterpretation of
/// [`ScopeId::hash_key`], identical across runs and platforms (ADR-012:
/// deterministic hashing).
#[must_use]
pub fn scope_key_i64(scope: ScopeId) -> i64 {
    i64::from_ne_bytes(scope.hash_key().to_ne_bytes())
}

/// Deterministic `ScopeId ↔ i64` intern table (see module docs).
#[derive(Debug, Clone, Default)]
pub struct ScopeInterner {
    map: HashMap<i64, ScopeId>,
}

impl ScopeInterner {
    /// Empty table.
    #[must_use]
    pub fn new() -> ScopeInterner {
        ScopeInterner::default()
    }

    /// Interns a scope, returning its deterministic `i64` key.
    pub fn intern(&mut self, scope: ScopeId) -> i64 {
        let key = scope_key_i64(scope);
        self.map.insert(key, scope);
        key
    }

    /// Resolves an interned key back to its scope. `None` for keys that were
    /// never interned — callers degrade to `Unknown`/diagnostic, never panic
    /// (`07-runtime.md` §9).
    #[must_use]
    pub fn resolve(&self, key: i64) -> Option<ScopeId> {
        self.map.get(&key).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intern_round_trips_and_is_deterministic() {
        let mut i = ScopeInterner::new();
        let s = ScopeId::vlan(100);
        let k = i.intern(s);
        assert_eq!(k, scope_key_i64(s), "intern key is the deterministic hash");
        assert_eq!(i.resolve(k), Some(s));
        assert_eq!(i.resolve(k.wrapping_add(1)), None);
        // Re-interning is idempotent.
        assert_eq!(i.intern(s), k);
    }
}
