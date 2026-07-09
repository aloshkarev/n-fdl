//! Minimal SARIF 2.1.0 emission for M0 (T-08).
//!
//! This intentionally emits only a compact skeleton:
//! `version`, `$schema`, one `run` with `tool.driver.name = "adgl"`, and
//! `results[]` from non-superseded problems.
//! Full schema coverage is intentionally deferred.

use std::fmt::Write as _;

use airpulse_dsl_types::{ScopeId, ScopeType, Severity};

use crate::Snapshot;

const SARIF_SCHEMA_URL: &str = "https://json.schemastore.org/sarif-2.1.0.json";

/// Converts an extracted snapshot into a minimal deterministic SARIF JSON
/// document.
///
/// Mapping notes:
/// - `result.ruleId = problem.sarif_id` (ADR-008).
/// - superseded problems are excluded from `results`.
/// - `partialFingerprints` keys follow ADR-008:
///   `{ scope, target, causes }`.
/// - `causes` carries cause kinds from problem evidence.
#[must_use]
pub fn to_sarif(snapshot: &Snapshot) -> String {
    let mut out = String::new();
    out.push('{');
    out.push_str("\"version\":\"2.1.0\",");
    out.push_str("\"$schema\":");
    push_json_string(&mut out, SARIF_SCHEMA_URL);
    out.push(',');
    out.push_str("\"runs\":[{\"tool\":{\"driver\":{\"name\":\"adgl\"}},\"results\":[");

    let mut first = true;
    for problem in &snapshot.problems {
        if problem.superseded {
            continue;
        }
        if !first {
            out.push(',');
        }
        first = false;

        out.push('{');
        out.push_str("\"ruleId\":");
        push_json_string(&mut out, problem.sarif_id.as_str());
        out.push(',');
        out.push_str("\"level\":");
        push_json_string(&mut out, sarif_level(problem.severity));
        out.push(',');
        out.push_str("\"message\":{\"text\":");
        let message = result_message(problem.kind.as_str(), problem.scope.scope_type());
        push_json_string(&mut out, &message);
        out.push_str("},");
        out.push_str("\"partialFingerprints\":{");
        out.push_str("\"scope\":");
        push_json_string(&mut out, scope_type_name(problem.scope.scope_type()));
        out.push(',');
        out.push_str("\"target\":");
        let target = fingerprint_target(problem.target);
        push_json_string(&mut out, &target);
        out.push(',');
        out.push_str("\"causes\":[");
        for (idx, cause_kind) in problem.cause_kinds.iter().enumerate() {
            if idx > 0 {
                out.push(',');
            }
            push_json_string(&mut out, cause_kind.as_str());
        }
        out.push_str("]}");
        out.push('}');
    }

    out.push_str("]}]}");
    out
}

fn result_message(problem_kind: &str, scope_type: ScopeType) -> String {
    format!(
        "{problem_kind} detected in {} scope",
        scope_type_name(scope_type)
    )
}

fn sarif_level(severity: Severity) -> &'static str {
    match severity {
        Severity::Critical | Severity::High => "error",
        Severity::Medium | Severity::Low => "warning",
        Severity::Recommended | Severity::Optional => "note",
    }
}

fn scope_type_name(scope: ScopeType) -> &'static str {
    match scope {
        ScopeType::Session => "session",
        ScopeType::Port => "port",
        ScopeType::ClientMac => "client_mac",
        ScopeType::Vlan => "vlan",
        ScopeType::AccessPoint => "access_point",
        ScopeType::Global => "global",
    }
}

fn fingerprint_target(scope: ScopeId) -> String {
    format!(
        "{}:{:016x}",
        scope_type_name(scope.scope_type()),
        scope.hash_key()
    )
}

fn push_json_string(out: &mut String, s: &str) {
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0C}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c <= '\u{1F}' => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::to_sarif;
    use crate::{ProblemView, Snapshot};
    use airpulse_dsl_types::{EventTime, ProblemKind, SarifId, ScopeId, Severity};

    #[test]
    fn escapes_control_and_special_characters() {
        let snapshot = Snapshot {
            causes: Vec::new(),
            problems: vec![ProblemView {
                scope: ScopeId::GLOBAL,
                kind: ProblemKind::new("bad \"kind\" \\\n\t"),
                target: ScopeId::GLOBAL,
                time: EventTime::from_millis(1),
                severity: Severity::Low,
                sarif_id: SarifId::new("id\"\\\n"),
                cause_kinds: vec![],
                superseded: false,
            }],
            audit: Vec::new(),
        };

        let json = to_sarif(&snapshot);
        assert!(json.contains("\\\"kind\\\""));
        assert!(json.contains("\\\\"));
        assert!(json.contains("\\n"));
        assert!(json.contains("\\t"));
    }
}
