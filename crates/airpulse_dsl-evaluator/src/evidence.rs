//! Evidence JSON extraction and PII redaction (`10-catalog-abi.md` §11, ADR-009).

use std::collections::{BTreeMap, BTreeSet};

use airpulse_dsl_catalog::{events, resolve_event};
use airpulse_dsl_store::{EdgeEndpoint, EventNode, GraphStore, ProblemNode, SubGraph};
use airpulse_dsl_types::ScopeId;

const REDACTED: &str = "<redacted>";

/// Collects event field values referenced by a problem's cause evidence edges.
#[must_use]
pub fn collect_problem_evidence(
    store: &GraphStore,
    scope: ScopeId,
    problem: &ProblemNode,
    part: &SubGraph,
) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let Some(ring) = store.ring(scope) else {
        return out;
    };
    for cause_id in &problem.evidence {
        let Some(cause) = part.causes.values().find(|node| node.id == *cause_id) else {
            continue;
        };
        for edge in &part.edges {
            if edge.dst != cause.id {
                continue;
            }
            let EdgeEndpoint::Event(event_id) = edge.src else {
                continue;
            };
            let Some(event) = ring.get(event_id) else {
                continue;
            };
            merge_event_fields(&mut out, event);
        }
    }
    out
}

fn merge_event_fields(out: &mut BTreeMap<String, String>, event: &EventNode) {
    let Some(schema) = resolve_event(event.event_type.as_str()) else {
        return;
    };
    for (field_idx, value) in event.fields() {
        let Some(field) = schema.fields.iter().find(|f| f.idx == *field_idx) else {
            continue;
        };
        out.insert(field.name.to_string(), value.to_string());
    }
    for (field_idx, values) in event.int_list_fields() {
        let Some(field) = schema.fields.iter().find(|f| f.idx == *field_idx) else {
            continue;
        };
        let rendered = values
            .iter()
            .map(i64::to_string)
            .collect::<Vec<_>>()
            .join(",");
        out.insert(field.name.to_string(), format!("[{rendered}]"));
    }
}

/// Returns every catalog field name marked `[pii]`.
#[must_use]
pub fn catalog_pii_field_names() -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for schema in events() {
        for field in schema.fields.iter().filter(|f| f.pii) {
            names.insert(field.name.to_string());
        }
    }
    names
}

/// Applies strict-privacy redaction to a flat evidence field map (ADR-009).
#[must_use]
pub fn redact_evidence_field_map(
    fields: &BTreeMap<String, String>,
    strict: bool,
) -> BTreeMap<String, String> {
    if !strict {
        return fields.clone();
    }
    let pii = catalog_pii_field_names();
    let mut redacted = BTreeMap::new();
    for (name, value) in fields {
        redacted.insert(
            name.clone(),
            if pii.contains(name) {
                REDACTED.to_string()
            } else {
                value.clone()
            },
        );
    }
    redacted
}

#[cfg(test)]
mod tests {
    use super::{catalog_pii_field_names, redact_evidence_field_map};
    use airpulse_dsl_catalog::{EventOrBindingType, resolve_metric_path};
    use airpulse_dsl_store::{EventNode, EventProvenance};
    use airpulse_dsl_types::{EventId, EventTime, EventType, ScopeId};
    use std::collections::BTreeMap;

    #[test]
    fn redacts_catalog_pii_fields_in_strict_mode() {
        assert!(catalog_pii_field_names().contains("dst_ip"));
        let mut raw = BTreeMap::new();
        raw.insert("dst_ip".to_string(), "167772161".to_string());
        raw.insert("segment_size".to_string(), "1500".to_string());
        let strict = redact_evidence_field_map(&raw, true);
        assert_eq!(strict.get("dst_ip"), Some(&"<redacted>".to_string()));
        assert_eq!(strict.get("segment_size"), Some(&"1500".to_string()));
        let open = redact_evidence_field_map(&raw, false);
        assert_eq!(open.get("dst_ip"), Some(&"167772161".to_string()));
    }

    #[test]
    fn event_field_merge_includes_dst_ip() {
        let scope = ScopeId::session((1, 443), (2, 80));
        let dst_idx = resolve_metric_path(
            EventOrBindingType::Event(&EventType::new("tcp.retransmission_burst")),
            "dst_ip",
        )
        .expect("dst_ip")
        .0;
        let event = EventNode::new(
            EventId::new(1),
            EventType::new("tcp.retransmission_burst"),
            EventTime::from_millis(1),
            scope,
            vec![(dst_idx, 0x0a00_0001)],
            EventProvenance::default(),
        );
        let mut raw = BTreeMap::new();
        super::merge_event_fields(&mut raw, &event);
        assert_eq!(raw.get("dst_ip"), Some(&"167772161".to_string()));
    }

    #[test]
    fn int_list_evidence_is_rendered_and_privacy_redacted() {
        let scope = ScopeId::access_point(1);
        let list_idx = resolve_metric_path(
            EventOrBindingType::Event(&EventType::new("wifi.deauth_burst")),
            "client_macs",
        )
        .expect("client_macs")
        .0;
        let event = EventNode::new(
            EventId::new(2),
            EventType::new("wifi.deauth_burst"),
            EventTime::from_millis(1),
            scope,
            vec![],
            EventProvenance::default(),
        )
        .with_int_list_field(list_idx, vec![3, 1, 2, 2]);
        let mut raw = BTreeMap::new();
        super::merge_event_fields(&mut raw, &event);
        assert_eq!(raw.get("client_macs"), Some(&"[1,2,3]".to_string()));
        let strict = redact_evidence_field_map(&raw, true);
        assert_eq!(strict.get("client_macs"), Some(&"<redacted>".to_string()));
    }
}
