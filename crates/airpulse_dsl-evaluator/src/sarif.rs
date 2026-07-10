//! Minimal SARIF 2.1.0 emission for M0 (T-08).
//!
//! This intentionally emits only a compact skeleton:
//! `version`, `$schema`, one `run` with `tool.driver.name = "adgl"`, and
//! `results[]` from non-superseded problems.
//! Full schema coverage is intentionally deferred.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use airpulse_dsl_types::{ScopeId, ScopeType, Severity};

use crate::Snapshot;
use crate::evidence::redact_evidence_field_map;
use crate::extract::ProblemView;

const SARIF_SCHEMA_URL: &str = "https://json.schemastore.org/sarif-2.1.0.json";

/// SARIF emission options (`10-catalog-abi.md` §11, ADR-009).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SarifOptions {
    /// When true, PII-marked evidence fields are replaced with `"<redacted>"`.
    pub strict_privacy: bool,
}

impl SarifOptions {
    /// Convenience constructor for strict-privacy mode.
    #[must_use]
    pub const fn strict() -> Self {
        Self {
            strict_privacy: true,
        }
    }
}

/// Converts an extracted snapshot into a minimal deterministic SARIF JSON
/// document with default (non-strict) privacy settings.
#[must_use]
pub fn to_sarif(snapshot: &Snapshot) -> String {
    to_sarif_with_options(snapshot, SarifOptions::default())
}

/// Converts an extracted snapshot into SARIF JSON, honoring privacy options.
#[must_use]
pub fn to_sarif_with_options(snapshot: &Snapshot, options: SarifOptions) -> String {
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
        write_result(&mut out, problem, options);
    }

    out.push_str("]}]}");
    out
}

fn write_result(out: &mut String, problem: &ProblemView, options: SarifOptions) {
    out.push('{');
    out.push_str("\"ruleId\":");
    push_json_string(out, problem.sarif_id.as_str());
    out.push(',');
    out.push_str("\"level\":");
    push_json_string(out, sarif_level(problem.severity));
    out.push(',');
    out.push_str("\"message\":{\"text\":");
    let message = result_message(problem.kind.as_str(), problem.scope.scope_type());
    push_json_string(out, &message);
    out.push_str("},");
    out.push_str("\"partialFingerprints\":{");
    out.push_str("\"scope\":");
    push_json_string(out, scope_type_name(problem.scope.scope_type()));
    out.push(',');
    out.push_str("\"target\":");
    let target = fingerprint_target(problem.target);
    push_json_string(out, &target);
    out.push(',');
    out.push_str("\"causes\":[");
    for (idx, cause_kind) in problem.cause_kinds.iter().enumerate() {
        if idx > 0 {
            out.push(',');
        }
        push_json_string(out, cause_kind.as_str());
    }
    out.push_str("]}");
    if !problem.evidence_fields.is_empty() {
        out.push(',');
        out.push_str("\"properties\":{\"evidence\":");
        write_evidence_object(out, &problem.evidence_fields, options);
        out.push('}');
    }
    out.push('}');
}

fn write_evidence_object(
    out: &mut String,
    fields: &BTreeMap<String, String>,
    options: SarifOptions,
) {
    let rendered = redact_evidence_field_map(fields, options.strict_privacy);
    out.push('{');
    for (idx, (name, value)) in rendered.iter().enumerate() {
        if idx > 0 {
            out.push(',');
        }
        push_json_string(out, name);
        out.push(':');
        push_json_string(out, value);
    }
    out.push('}');
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
    use super::{SarifOptions, to_sarif_with_options};
    use crate::{ProblemView, Snapshot};
    use airpulse_dsl_types::{EventTime, ProblemKind, SarifId, ScopeId, Severity};
    use std::collections::BTreeMap;

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
                evidence_fields: BTreeMap::new(),
                superseded: false,
            }],
            audit: Vec::new(),
        };

        let json = to_sarif_with_options(&snapshot, SarifOptions::default());
        assert!(json.contains("\\\"kind\\\""));
        assert!(json.contains("\\\\"));
        assert!(json.contains("\\n"));
        assert!(json.contains("\\t"));
    }

    #[test]
    fn strict_privacy_redacts_pii_evidence_fields() {
        let mut fields = BTreeMap::new();
        fields.insert("dst_ip".to_string(), "167772161".to_string());
        fields.insert("segment_size".to_string(), "1500".to_string());
        let open_snapshot = Snapshot {
            causes: Vec::new(),
            problems: vec![ProblemView {
                scope: ScopeId::GLOBAL,
                kind: ProblemKind::new("XlIcmpTcpMss"),
                target: ScopeId::GLOBAL,
                time: EventTime::from_millis(1),
                severity: Severity::High,
                sarif_id: SarifId::new("l3_pmtud_blackhole"),
                cause_kinds: vec![],
                evidence_fields: fields,
                superseded: false,
            }],
            audit: Vec::new(),
        };

        let open = to_sarif_with_options(&open_snapshot, SarifOptions::default());
        assert!(open.contains("\"dst_ip\":\"167772161\""));
        assert!(open.contains("\"segment_size\":\"1500\""));

        let strict = to_sarif_with_options(&open_snapshot, SarifOptions::strict());
        assert!(strict.contains("\"dst_ip\":\"<redacted>\""));
        assert!(strict.contains("\"segment_size\":\"1500\""));
    }
}
