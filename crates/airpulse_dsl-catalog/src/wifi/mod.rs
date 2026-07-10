//! Wi-Fi catalog v1.3 — bounded raw/FSM/summary vocabulary.
//!
//! **Event budget (W1):** 13 raw management/control/data events, 9 FSM outcome
//! events, 2 aggregate summaries (`wifi.ap.summary`, `wifi.client.summary`),
//! and 4 special stream events (`wifi.deauth_burst`, `wifi.rf_telemetry`,
//! `tcp.rtt_sample`, `tcp.throughput_sample`) already present or added here.
//! The ninth outcome,
//! `wifi.client.twt_outcome`, exists only for the in-scope cross-layer TWT/RTO
//! correlation and is never a standalone ClientIssue.

mod events_data;
mod mappings;

use airpulse_dsl_types::ScopeType;

pub(crate) use events_data::WIFI_EVENT_SPECS;
pub use mappings::{WIFI_MAPPINGS, WifiMapping};
pub(crate) use mappings::{wifi_causes, wifi_problems};

use crate::EventSchema;
use crate::FieldSchema;
use crate::FieldType;
use crate::ScopeRoute;
use crate::helpers::{event, idx, int_field};
use events_data::WifiEventSpec;

/// Builds all Wi-Fi catalog events from [`WIFI_EVENT_SPECS`].
pub(crate) fn wifi_events() -> Box<[EventSchema]> {
    WIFI_EVENT_SPECS.iter().map(build_wifi_event).collect()
}

fn build_wifi_event(spec: &WifiEventSpec) -> EventSchema {
    let mut fields = vec![scoped_target(spec.scope), int_field("time", 1)];
    let mut next_idx = 2u16;
    for name in spec.pii_fields {
        fields.push(FieldSchema {
            name,
            field_type: FieldType::Int,
            pii: true,
            idx: idx(next_idx),
        });
        next_idx += 1;
    }
    for name in spec.extra_fields {
        fields.push(int_field(name, next_idx));
        next_idx += 1;
    }
    // W2 appends sibling-scope routing fields without moving v1.3 scalar indices.
    if spec.name == "wifi.mgmt.deauth" {
        fields.push(FieldSchema {
            name: "bssid",
            field_type: FieldType::Int,
            pii: true,
            idx: idx(next_idx),
        });
        next_idx += 1;
        fields.push(scoped_field("ap_target", ScopeType::AccessPoint, next_idx));
        next_idx += 1;
    }
    if spec.name == "wifi.mgmt.action" {
        fields.push(scoped_field(
            "client_target",
            ScopeType::ClientMac,
            next_idx,
        ));
    }
    let routes = match spec.name {
        "wifi.mgmt.deauth" => &[
            ScopeRoute {
                scope: ScopeType::ClientMac,
                path: "target",
            },
            ScopeRoute {
                scope: ScopeType::AccessPoint,
                path: "ap_target",
            },
        ][..],
        "wifi.mgmt.action" => &[
            ScopeRoute {
                scope: ScopeType::AccessPoint,
                path: "target",
            },
            ScopeRoute {
                scope: ScopeType::ClientMac,
                path: "client_target",
            },
        ][..],
        _ => &[ScopeRoute {
            scope: spec.scope,
            path: "target",
        }][..],
    };
    event(spec.name, &fields, routes)
}

fn scoped_target(scope: ScopeType) -> FieldSchema {
    FieldSchema {
        name: "target",
        field_type: FieldType::ScopeId(scope),
        pii: false,
        idx: idx(0),
    }
}

fn scoped_field(name: &'static str, scope: ScopeType, index: u16) -> FieldSchema {
    FieldSchema {
        name,
        field_type: FieldType::ScopeId(scope),
        pii: false,
        idx: idx(index),
    }
}
