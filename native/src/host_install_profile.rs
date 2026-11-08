//! Opt-in, thread-local diagnostics compiled only into Windows unit tests.
use std::{cell::RefCell, time::Instant};

#[derive(Clone, Copy)]
#[repr(usize)]
pub(in crate::host_install) enum Phase {
    Load,
    LoadNames,
    EventRead,
    PlanRead,
    EventValidate,
    Chain,
    Append,
    Prepare,
    Metadata,
    Names,
    Reserve,
    Immutable,
    SourceSecurity,
    FreshSecurity,
}

const LABELS: [&str; 14] = [
    "load",
    "loadNames",
    "eventRead",
    "planRead",
    "eventValidate",
    "chain",
    "append",
    "prepare",
    "metadata",
    "names",
    "reserve",
    "immutable",
    "sourceSecurity",
    "freshSecurity",
];

#[derive(Clone, Copy, Default)]
struct Metric {
    calls: u64,
    nanos: u128,
    max_nanos: u128,
}

struct State {
    enabled: bool,
    started: Option<Instant>,
    metrics: [Metric; LABELS.len()],
}
impl Default for State {
    fn default() -> Self {
        Self {
            enabled: false,
            started: None,
            metrics: [Metric::default(); LABELS.len()],
        }
    }
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

pub(in crate::host_install) struct Session;
impl Drop for Session {
    fn drop(&mut self) {
        STATE.with(|state| state.borrow_mut().enabled = false);
    }
}

pub(in crate::host_install) fn start() -> Session {
    STATE.with(|state| {
        *state.borrow_mut() = State {
            enabled: true,
            started: Some(Instant::now()),
            ..State::default()
        };
    });
    Session
}

pub(in crate::host_install) struct Span {
    phase: Phase,
    started: Option<Instant>,
}
impl Drop for Span {
    fn drop(&mut self) {
        let Some(started) = self.started else { return };
        let elapsed = started.elapsed().as_nanos();
        STATE.with(|state| {
            let mut state = state.borrow_mut();
            if state.enabled {
                let metric = &mut state.metrics[self.phase as usize];
                metric.calls = metric.calls.saturating_add(1);
                metric.nanos = metric.nanos.saturating_add(elapsed);
                metric.max_nanos = metric.max_nanos.max(elapsed);
            }
        });
    }
}

pub(in crate::host_install) fn span(phase: Phase) -> Span {
    Span {
        phase,
        started: STATE.with(|state| state.borrow().enabled.then(Instant::now)),
    }
}

// Inclusive span durations overlap and must not be summed. There are only
// fixed numeric fields and labels: no source, paths or security bytes.
pub(in crate::host_install) fn report() -> String {
    STATE.with(|state| {
        let state = state.borrow();
        let rows = LABELS
            .iter()
            .zip(&state.metrics)
            .map(|(label, metric)| {
                serde_json::json!({
                    "phase": label,
                    "calls": metric.calls,
                    "inclusiveMicros": metric.nanos / 1_000,
                    "maxMicros": metric.max_nanos / 1_000,
                })
            })
            .collect::<Vec<_>>();
        serde_json::json!({
            "elapsedMicros": state.started.map(|s| s.elapsed().as_micros()).unwrap_or(0),
            "inclusiveSpans": rows,
        })
        .to_string()
    })
}
