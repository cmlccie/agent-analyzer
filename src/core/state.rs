use crate::core::target::{Target, TargetKind};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Shared, mutable application state. Owned by the `serve` process and read
/// by HTTP handlers, discovery tasks, and probe tasks.
#[derive(Debug, Default, Clone)]
pub struct AppState {
    inner: Arc<RwLock<StateInner>>,
}

#[derive(Debug, Default)]
struct StateInner {
    targets: HashMap<String, Target>,
}

impl AppState {
    pub fn new() -> Self {
        AppState::default()
    }

    /// Insert or replace a target.
    pub fn upsert(&self, target: Target) {
        self.inner
            .write()
            .expect("state lock poisoned")
            .targets
            .insert(target.id.clone(), target);
    }

    /// Remove a target by id.
    pub fn remove(&self, id: &str) {
        self.inner
            .write()
            .expect("state lock poisoned")
            .targets
            .remove(id);
    }

    /// Snapshot all targets.
    pub fn all(&self) -> Vec<Target> {
        self.inner
            .read()
            .expect("state lock poisoned")
            .targets
            .values()
            .cloned()
            .collect()
    }

    /// Snapshot targets filtered by kind.
    pub fn by_kind(&self, kind: TargetKind) -> Vec<Target> {
        self.inner
            .read()
            .expect("state lock poisoned")
            .targets
            .values()
            .filter(|t| t.kind == kind)
            .cloned()
            .collect()
    }

    /// Look up a single target by id.
    pub fn get(&self, id: &str) -> Option<Target> {
        self.inner
            .read()
            .expect("state lock poisoned")
            .targets
            .get(id)
            .cloned()
    }

    pub fn target_count(&self) -> usize {
        self.inner
            .read()
            .expect("state lock poisoned")
            .targets
            .len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::target::{Source, Status, Target, TargetKind};

    fn make_target(id: &str, kind: TargetKind) -> Target {
        Target {
            id: id.to_string(),
            kind,
            name: id.to_string(),
            url: "http://localhost".parse().unwrap(),
            source: Source::Manual,
            status: Status::Unknown,
            metadata: Default::default(),
        }
    }

    #[test]
    fn upsert_and_get() {
        let state = AppState::new();
        let t = make_target("m1", TargetKind::Model);
        state.upsert(t.clone());
        let got = state.get("m1").unwrap();
        assert_eq!(got.id, "m1");
    }

    #[test]
    fn remove() {
        let state = AppState::new();
        state.upsert(make_target("m1", TargetKind::Model));
        state.remove("m1");
        assert!(state.get("m1").is_none());
    }

    #[test]
    fn by_kind() {
        let state = AppState::new();
        state.upsert(make_target("m1", TargetKind::Model));
        state.upsert(make_target("a1", TargetKind::Agent));
        state.upsert(make_target("t1", TargetKind::Tool));

        assert_eq!(state.by_kind(TargetKind::Model).len(), 1);
        assert_eq!(state.by_kind(TargetKind::Agent).len(), 1);
        assert_eq!(state.by_kind(TargetKind::Website).len(), 0);
    }

    #[test]
    fn all_returns_all() {
        let state = AppState::new();
        state.upsert(make_target("m1", TargetKind::Model));
        state.upsert(make_target("m2", TargetKind::Model));
        assert_eq!(state.all().len(), 2);
    }
}
