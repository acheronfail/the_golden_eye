//! Serializes work admission with update and shutdown decisions.
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
pub(crate) struct CoreLifecycle(Arc<Mutex<State>>);

#[derive(Default)]
struct State {
    closing: bool,
    active: [usize; 3],
}

#[derive(Clone, Copy)]
pub(crate) enum WorkKind {
    Monitor,
    ReplaySave,
    FolderPicker,
}

pub(crate) struct WorkPermit {
    lifecycle: CoreLifecycle,
    kind: WorkKind,
}

impl CoreLifecycle {
    pub(crate) fn start(&self, kind: WorkKind) -> Option<WorkPermit> {
        let mut state = self.0.lock().unwrap_or_else(|p| p.into_inner());
        if state.closing {
            return None;
        }
        state.active[kind as usize] += 1;
        Some(WorkPermit { lifecycle: self.clone(), kind })
    }

    pub(crate) fn begin_update(&self, allow_monitor: bool) -> bool {
        let mut state = self.0.lock().unwrap_or_else(|p| p.into_inner());
        if state.closing
            || state.active[1..].iter().any(|count| *count != 0)
            || (!allow_monitor && state.active[0] != 0)
        {
            return false;
        }
        state.closing = true;
        true
    }

    pub(crate) fn close(&self) {
        self.0.lock().unwrap_or_else(|p| p.into_inner()).closing = true;
    }

    pub(crate) fn is_closing(&self) -> bool {
        self.0.lock().unwrap_or_else(|p| p.into_inner()).closing
    }
}

impl Drop for WorkPermit {
    fn drop(&mut self) {
        self.lifecycle.0.lock().unwrap_or_else(|p| p.into_inner()).active[self.kind as usize] -= 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_waits_for_each_dependency_and_closes_admission() {
        for kind in [WorkKind::Monitor, WorkKind::ReplaySave, WorkKind::FolderPicker] {
            let lifecycle = CoreLifecycle::default();
            let work = lifecycle.start(kind).unwrap();
            assert!(!lifecycle.begin_update(false));
            drop(work);
            assert!(lifecycle.begin_update(false));
            assert!(lifecycle.start(kind).is_none());
            assert!(!lifecycle.begin_update(false));
        }
    }

    #[test]
    fn dev_reload_only_bypasses_monitor_activity() {
        let lifecycle = CoreLifecycle::default();
        let _monitor = lifecycle.start(WorkKind::Monitor).unwrap();
        let save = lifecycle.start(WorkKind::ReplaySave).unwrap();
        assert!(!lifecycle.begin_update(true));
        drop(save);
        assert!(lifecycle.begin_update(true));
    }
}
