//! Scope types, the scope lattice (`⊑`), and `ScopeId` partition keys per
//! `docs/idea/spec/04-type-system.md` §5 and
//! `docs/idea/spec/09-scopes-sessions.md` §3/§7.

/// The six scope (partition) types, `04-type-system.md` §5:
/// `ScopeType ::= Session | Port | ClientMac | Vlan | AccessPoint | Global`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ScopeType {
    /// TCP 5-tuple session (canonical bidirectional tuple, `09` §7).
    Session,
    /// Physical switch port `(switch_id, port_id)`.
    Port,
    /// Client MAC address.
    ClientMac,
    /// VLAN id.
    Vlan,
    /// Access point (BSSID).
    AccessPoint,
    /// Singleton global partition (`09` §6).
    Global,
}

impl ScopeType {
    /// Direct parent in the scope hierarchy (`04` §5 / `09` §3.1):
    ///
    /// ```text
    /// ClientMac ⊂ Vlan ⊂ Global
    /// Session   ⊂ Vlan
    /// Port      ⊂ Global
    /// AccessPoint ⊂ Global
    /// ```
    #[must_use]
    pub const fn parent(self) -> Option<ScopeType> {
        match self {
            ScopeType::Session | ScopeType::ClientMac => Some(ScopeType::Vlan),
            ScopeType::Vlan | ScopeType::Port | ScopeType::AccessPoint => Some(ScopeType::Global),
            ScopeType::Global => None,
        }
    }

    /// Partial order `self ⊑ other` (subsumption): `other` is `self` or an
    /// ancestor of `self` in the hierarchy above.
    ///
    /// This is the relation used by the `rule.scope ⊑ target`-scope typing
    /// rule (`04-type-system.md` §7 T-Infer/T-Emit; `09-scopes-sessions.md`
    /// §4): same-scope and child-rule/parent-target roll-up are allowed,
    /// sibling or rule-in-parent/target-in-child are not.
    #[must_use]
    pub const fn is_subsumed_by(self, other: ScopeType) -> bool {
        let mut cur = self;
        loop {
            if cur as u8 == other as u8 {
                return true;
            }
            match cur.parent() {
                Some(p) => cur = p,
                None => return false,
            }
        }
    }

    /// Reverse of [`ScopeType::is_subsumed_by`]: `other ⊑ self`.
    #[must_use]
    pub const fn subsumes(self, other: ScopeType) -> bool {
        other.is_subsumed_by(self)
    }
}

/// Deterministic partition key: `ScopeId = hash(ScopeType, key_components)`
/// (`04-type-system.md` §5; canonical key components per `09` §7).
///
/// The key hash is a fixed FNV-1a 64 over canonical key components, so it is
/// identical across runs and platforms — required for deterministic
/// cross-partition ordering (`09-scopes-sessions.md` §5: "scope_id_hash —
/// deterministic, same hash function on every run").
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ScopeId {
    scope_type: ScopeType,
    key: u64,
}

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// FNV-1a 64 over a byte slice; deterministic, dependency-free.
const fn fnv1a(mut hash: u64, bytes: &[u8]) -> u64 {
    let mut i = 0;
    while i < bytes.len() {
        hash ^= bytes[i] as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
        i += 1;
    }
    hash
}

const fn fnv1a_u64(hash: u64, v: u64) -> u64 {
    fnv1a(hash, &v.to_be_bytes())
}

const fn fnv1a_u128(hash: u64, v: u128) -> u64 {
    fnv1a(hash, &v.to_be_bytes())
}

/// One session endpoint: `(ip, port)`. IPv4 addresses are embedded into the
/// `u128` IP space (IPv4-mapped), so one representation covers both families.
pub type SessionEndpoint = (u128, u16);

impl ScopeId {
    /// The singleton `Global` partition key: `ScopeId(GLOBAL, ())`
    /// (`09-scopes-sessions.md` §6; `04` §5 "Global — singleton partition").
    pub const GLOBAL: ScopeId = ScopeId { scope_type: ScopeType::Global, key: 0 };

    /// `Session = bidir_tuple((ip, port), (ip, port))` (`09` §7): the two
    /// endpoints are sorted *atomically* (IP and port together, mirroring
    /// N-FDL ADR-008), so both flow directions map to the same partition.
    #[must_use]
    pub fn session(a: SessionEndpoint, b: SessionEndpoint) -> ScopeId {
        let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
        let mut h = FNV_OFFSET;
        h = fnv1a_u128(h, lo.0);
        h = fnv1a_u64(h, lo.1 as u64);
        h = fnv1a_u128(h, hi.0);
        h = fnv1a_u64(h, hi.1 as u64);
        ScopeId { scope_type: ScopeType::Session, key: h }
    }

    /// `Port = (switch_id, port_id)` (`09` §7).
    #[must_use]
    pub fn port(switch_id: u64, port_id: u32) -> ScopeId {
        let mut h = FNV_OFFSET;
        h = fnv1a_u64(h, switch_id);
        h = fnv1a_u64(h, port_id as u64);
        ScopeId { scope_type: ScopeType::Port, key: h }
    }

    /// `ClientMac = client_mac` (`09` §7). The MAC's 48 bits in the low bytes.
    #[must_use]
    pub fn client_mac(mac: u64) -> ScopeId {
        ScopeId { scope_type: ScopeType::ClientMac, key: fnv1a_u64(FNV_OFFSET, mac) }
    }

    /// `Vlan = vlan_id` (`09` §7).
    #[must_use]
    pub fn vlan(vlan_id: u16) -> ScopeId {
        ScopeId { scope_type: ScopeType::Vlan, key: fnv1a_u64(FNV_OFFSET, vlan_id as u64) }
    }

    /// `AccessPoint = bssid` (`09` §7). The BSSID's 48 bits in the low bytes.
    #[must_use]
    pub fn access_point(bssid: u64) -> ScopeId {
        ScopeId { scope_type: ScopeType::AccessPoint, key: fnv1a_u64(FNV_OFFSET, bssid) }
    }

    /// The scope (partition) type of this key.
    #[must_use]
    pub const fn scope_type(self) -> ScopeType {
        self.scope_type
    }

    /// Deterministic 64-bit hash used for cross-partition tie-breaking
    /// (`09-scopes-sessions.md` §5 `scope_id_hash`).
    #[must_use]
    pub const fn hash_key(self) -> u64 {
        // Fold the scope type in so ids of different types never collide on 0.
        fnv1a_u64(fnv1a(FNV_OFFSET, &[self.scope_type as u8]), self.key)
    }

    /// Scope-type-level subsumption `self ⊑ other`: this id's scope type is
    /// equal to or a descendant of `other`'s (`04` §7, `09` §4). Identity of
    /// the *key* mapping (child key → parent key, e.g. ClientMac → Vlan via
    /// the DHCP event's vlan id) is event-data-driven and owned by the store
    /// (`09` §3.1), not derivable here.
    #[must_use]
    pub const fn is_subsumed_by(self, other: ScopeId) -> bool {
        self.scope_type.is_subsumed_by(other.scope_type)
    }
}

#[cfg(test)]
mod tests {
    use super::ScopeType::{AccessPoint, ClientMac, Global, Port, Session, Vlan};
    use super::*;

    const ALL: [ScopeType; 6] = [Session, Port, ClientMac, Vlan, AccessPoint, Global];

    #[test]
    fn subsumption_is_reflexive() {
        // ⊑ is a partial order: σ ⊑ σ for every scope type (09 §4 allows
        // same-scope, e.g. Session/Session).
        for s in ALL {
            assert!(s.is_subsumed_by(s), "{s:?} ⊑ {s:?} must hold");
        }
    }

    #[test]
    fn subsumption_is_antisymmetric() {
        // a ⊑ b and b ⊑ a ⇒ a == b.
        for a in ALL {
            for b in ALL {
                if a.is_subsumed_by(b) && b.is_subsumed_by(a) {
                    assert_eq!(a, b, "antisymmetry violated for {a:?}, {b:?}");
                }
            }
        }
    }

    #[test]
    fn subsumption_is_transitive() {
        for a in ALL {
            for b in ALL {
                for c in ALL {
                    if a.is_subsumed_by(b) && b.is_subsumed_by(c) {
                        assert!(a.is_subsumed_by(c), "transitivity: {a:?} ⊑ {b:?} ⊑ {c:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn hierarchy_matches_spec() {
        // 04 §5 / 09 §3.1 literal hierarchy.
        assert!(ClientMac.is_subsumed_by(Vlan));
        assert!(Session.is_subsumed_by(Vlan));
        assert!(Vlan.is_subsumed_by(Global));
        assert!(Port.is_subsumed_by(Global));
        assert!(AccessPoint.is_subsumed_by(Global));
        // Transitive closure through Vlan.
        assert!(ClientMac.is_subsumed_by(Global));
        assert!(Session.is_subsumed_by(Global));
        // Everything ⊑ Global; Global ⊑ nothing else.
        for s in ALL {
            assert!(s.is_subsumed_by(Global));
            if s != Global {
                assert!(!Global.is_subsumed_by(s));
            }
        }
    }

    #[test]
    fn siblings_and_descendants_are_not_subsumed() {
        // 09 §4: target may not be a sibling or a descendant of rule.scope.
        assert!(!Session.is_subsumed_by(Port)); // siblings (incomparable)
        assert!(!Port.is_subsumed_by(Session));
        assert!(!Session.is_subsumed_by(ClientMac)); // same parent, incomparable
        assert!(!Vlan.is_subsumed_by(ClientMac)); // parent ⋢ child
        assert!(!Vlan.is_subsumed_by(Session));
        assert!(!Port.is_subsumed_by(Vlan)); // Port is not under Vlan
        assert!(!AccessPoint.is_subsumed_by(Vlan));
    }

    #[test]
    fn subsumes_is_inverse_of_is_subsumed_by() {
        for a in ALL {
            for b in ALL {
                assert_eq!(a.subsumes(b), b.is_subsumed_by(a));
            }
        }
    }

    #[test]
    fn session_key_is_bidirectional() {
        // 09 §7 bidir_tuple: endpoints sorted atomically — both directions of
        // the same 5-tuple map to the same ScopeId.
        let a = (0x0a00_0001_u128, 443_u16);
        let b = (0x0a00_0002_u128, 51234_u16);
        assert_eq!(ScopeId::session(a, b), ScopeId::session(b, a));
        // IP and port sort as a unit, not independently: swapping ports across
        // endpoints produces a *different* session.
        let a2 = (0x0a00_0001_u128, 51234_u16);
        let b2 = (0x0a00_0002_u128, 443_u16);
        assert_ne!(ScopeId::session(a, b), ScopeId::session(a2, b2));
    }

    #[test]
    fn scope_ids_are_deterministic_and_distinct() {
        assert_eq!(ScopeId::vlan(100), ScopeId::vlan(100));
        assert_ne!(ScopeId::vlan(100), ScopeId::vlan(200));
        assert_ne!(ScopeId::port(1, 2), ScopeId::port(2, 1));
        // Same numeric key under different scope types must not compare equal.
        assert_ne!(ScopeId::client_mac(42), ScopeId::access_point(42));
        assert_ne!(ScopeId::client_mac(42).hash_key(), ScopeId::access_point(42).hash_key());
    }

    #[test]
    fn global_is_singleton() {
        assert_eq!(ScopeId::GLOBAL, ScopeId::GLOBAL);
        assert_eq!(ScopeId::GLOBAL.scope_type(), Global);
    }

    #[test]
    fn scope_id_subsumption_follows_type_lattice() {
        let mac = ScopeId::client_mac(0xAA_BB_CC_DD_EE_FF);
        let vlan = ScopeId::vlan(7);
        assert!(mac.is_subsumed_by(vlan));
        assert!(mac.is_subsumed_by(mac)); // reflexive on ids
        assert!(!vlan.is_subsumed_by(mac));
        assert!(vlan.is_subsumed_by(ScopeId::GLOBAL));
    }
}
