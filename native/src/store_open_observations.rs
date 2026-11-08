//! Explicit host-only diagnostics. These observations grant no state authority.
use std::{
    fmt,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum OpeningPhase {
    Budget,
    StateDirectory,
    StateSubdirectories,
    MaintenanceFile,
    ClientFile,
    AdmissionLock,
    ClientLock,
    AdmissionUnlock,
    DatabaseFiles,
    Connection,
    SqlPolicy,
    SqlSetup,
    WorkspaceIdentity,
    ClearPolicy,
    Ready,
}

const PHASES: usize = 15;

/// One Store opening's first phase-entry samples, reset by each observed call.
/// A phase is an entered boundary, not proof of completion or the failing OS API.
/// Values contain no paths, SQL, source content, error text or absolute clocks.
pub struct OpeningObservations {
    started: Instant,
    last: Option<OpeningPhase>,
    first: [Option<(OpeningPhase, Duration)>; PHASES],
}

impl Default for OpeningObservations {
    fn default() -> Self {
        Self {
            started: Instant::now(),
            last: None,
            first: [None; PHASES],
        }
    }
}

impl fmt::Debug for OpeningObservations {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StoreOpeningObservations")
            .field("last", &self.last)
            .field("first", &self.first)
            .field("elapsed", &self.started.elapsed())
            .finish()
    }
}

impl OpeningObservations {
    pub fn last_phase(&self) -> Option<OpeningPhase> {
        self.last
    }

    pub fn first_observation(&self, phase: OpeningPhase) -> Option<Duration> {
        self.first[phase as usize].map(|(_, elapsed)| elapsed)
    }

    pub(super) fn reset(&mut self) {
        *self = Self::default();
    }

    fn enter(&mut self, phase: OpeningPhase) {
        let slot = &mut self.first[phase as usize];
        if slot.is_none() {
            *slot = Some((phase, self.started.elapsed()));
        }
        self.last = Some(phase);
    }
}

pub(super) fn enter(observations: &mut Option<&mut OpeningObservations>, phase: OpeningPhase) {
    if let Some(observations) = observations {
        observations.enter(phase);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_samples_survive_reentry_and_reset_clears_the_prior_opening() {
        let mut observations = OpeningObservations::default();
        observations.enter(OpeningPhase::Budget);
        let first = observations.first_observation(OpeningPhase::Budget);
        observations.enter(OpeningPhase::SqlSetup);
        observations.enter(OpeningPhase::Budget);
        assert_eq!(observations.first_observation(OpeningPhase::Budget), first);
        assert_eq!(observations.last_phase(), Some(OpeningPhase::Budget));
        observations.reset();
        assert_eq!(observations.last_phase(), None);
        assert!(observations.first.iter().all(Option::is_none));
    }

    #[test]
    fn debug_excludes_absolute_clock_and_storage_remains_fixed_size() {
        let mut observations = OpeningObservations::default();
        enter(&mut Some(&mut observations), OpeningPhase::SqlSetup);
        let debug = format!("{observations:?}");
        assert!(debug.contains("SqlSetup"));
        assert!(!debug.contains("Instant"));
        assert!(std::mem::size_of::<OpeningObservations>() <= 1024);
        enter(&mut None, OpeningPhase::Ready);
        assert_eq!(observations.last_phase(), Some(OpeningPhase::SqlSetup));
    }
}
