//! Catalog publish gate: an infer becomes a cause only when `resolve_cause`
//! accepts the kind, the target type is in `valid_scopes`, and evidence is
//! non-empty. The published id is kind + scope + target.

use airpulse_dsl_evaluator::schema::EVENT_FIELD_TARGET;
use airpulse_dsl_evaluator::{
    Engine, EngineDiagnostic, OfflineAuditSink, RunMode, StaticTopology, to_sarif,
};
use airpulse_dsl_ir::{
    AnchorSource, AnchorSpec, CatalogRef, Intent, Predicate, ProgramImage, ProvKey, RuleInstance,
    RuleKind, Symbol, VerifiedAnnotations,
};
use airpulse_dsl_store::{EventNode, EventProvenance, Limits};
use airpulse_dsl_types::{
    CauseKind, EventId, EventTime, EventType, EvidenceState, MetricPath, RuleId, ScopeId,
    ScopeType, Weight,
};

fn image(kind: &str, state: EvidenceState) -> ProgramImage {
    let cause = CauseKind::new(kind);
    let rule = RuleInstance {
        id: RuleId::new("publish_gate"),
        kind: RuleKind::Evidence,
        scope: ScopeType::Global,
        anchor: AnchorSpec {
            binding: Symbol::new("ev"),
            source: AnchorSource::Event(EventType::new("net.device_unreachable")),
            predicate: Predicate::always_true(),
        },
        correlates: Box::new([]),
        branches: None,
        body: Box::new([Intent::InferCause {
            cause: cause.clone(),
            target: MetricPath::new("ev.target"),
            weight: Weight::new(100).expect("weight"),
            evidence: Box::new([Symbol::new("ev")]),
            evidence_state: Box::new([state]),
            provenance_key: ProvKey {
                rule: RuleId::new("publish_gate"),
                cause,
                target_expr_hash: 1,
            },
            evidence_pii: Box::new([]),
        }]),
        annotations: VerifiedAnnotations::from_correlates(&[], None),
    };
    let requires: Box<[airpulse_dsl_types::Capability]> = Box::from([]);
    let exclusivity: Box<[airpulse_dsl_ir::ExclusivityGroup]> = Box::from([]);
    ProgramImage::new(
        ProgramImage::pack_version(1, 0, 0),
        "publish.gate",
        requires,
        exclusivity,
        Box::new([rule]),
        CatalogRef {
            id: "airpulse.catalog".into(),
            version: "1.0".into(),
        },
    )
}

fn ingest(img: &ProgramImage) -> Engine<'_, StaticTopology, OfflineAuditSink> {
    let mut eng = Engine::new(
        img,
        StaticTopology::new(Limits::default().max_topology_hops),
        OfflineAuditSink::new(),
        Limits::default(),
        RunMode::Offline,
    );
    let port = ScopeId::port(7, 1);
    let key = eng.intern_scope(port);
    eng.ingest(EventNode::new(
        EventId::new(1),
        EventType::new("net.device_unreachable"),
        EventTime::from_millis(1_000),
        ScopeId::GLOBAL,
        vec![(EVENT_FIELD_TARGET, key)],
        EventProvenance::default(),
    ));
    eng.finish();
    eng
}

#[test]
fn unknown_kind_is_not_published() {
    let img = image("NotACatalogCause", EvidenceState::Present);
    let eng = ingest(&img);
    assert!(eng.snapshot().causes.is_empty());
    assert!(eng.diagnostics().iter().any(|d| {
        matches!(d, EngineDiagnostic::UnknownCause { .. }) && d.code() == Some("ADGL3104")
    }));
}

#[test]
fn target_outside_valid_scopes_is_not_published() {
    // UpstreamOutage is Global-only. The event target is a port.
    let img = image("UpstreamOutage", EvidenceState::Present);
    let eng = ingest(&img);
    assert!(eng.snapshot().causes.is_empty());
    assert!(eng.diagnostics().iter().any(|d| {
        matches!(d, EngineDiagnostic::CauseScopeRejected { .. }) && d.code() == Some("ADGL3105")
    }));
}

#[test]
fn catalog_cause_keeps_absent_evidence_and_published_id() {
    let img = image("PhysicalLinkAbsent", EvidenceState::Absent);
    let eng = ingest(&img);
    let snap = eng.snapshot();
    assert_eq!(snap.causes.len(), 1);
    let cause = &snap.causes[0];
    assert_eq!(cause.kind.as_str(), "PhysicalLinkAbsent");
    assert_eq!(cause.target, ScopeId::port(7, 1));
    assert_eq!(cause.evidence_refs.len(), 1);
    assert_eq!(cause.evidence_refs[0].0.as_ref(), "ev");
    assert_eq!(cause.evidence_refs[0].1, EvidenceState::Absent);
    assert!(cause.published_id.starts_with("PhysicalLinkAbsent@global:"));
    assert!(cause.published_id.contains(":port:"));
    let sarif = to_sarif(&snap);
    assert!(sarif.contains("coverage=partial"));
    assert!(sarif.contains("\"state\":\"absent\""));
    assert!(sarif.contains(&cause.published_id));
    assert!(!sarif.contains("NotACatalogCause"));
}
