use crate::core::target::{Status, Target, TargetKind};
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

    /// Insert a freshly discovered target, keeping the status of any target already known by
    /// that id. Discovery re-runs periodically and always produces `Status::Unknown`; letting it
    /// overwrite would flip a probed target back to `?` and drop its last error.
    pub fn upsert_discovered(&self, mut target: Target) {
        let mut inner = self.inner.write().expect("state lock poisoned");
        if let Some(existing) = inner.targets.get(&target.id) {
            target.status = existing.status.clone();
        }
        inner.targets.insert(target.id.clone(), target);
    }

    /// Record a probe result. Only the status is touched, so metadata refreshed by a concurrent
    /// discovery pass is kept, and a target removed while its probe was in flight stays removed.
    pub fn set_status(&self, id: &str, status: Status) {
        if let Some(t) = self
            .inner
            .write()
            .expect("state lock poisoned")
            .targets
            .get_mut(id)
        {
            t.status = status;
        }
    }

    /// Remove a target by id.
    pub fn remove(&self, id: &str) {
        self.inner
            .write()
            .expect("state lock poisoned")
            .targets
            .remove(id);
    }

    /// Snapshot all targets, ordered by name so clients render a stable list.
    pub fn all(&self) -> Vec<Target> {
        self.snapshot(|_| true)
    }

    /// Snapshot targets filtered by kind, ordered by name.
    pub fn by_kind(&self, kind: TargetKind) -> Vec<Target> {
        self.snapshot(|t| t.kind == kind)
    }

    fn snapshot(&self, keep: impl Fn(&Target) -> bool) -> Vec<Target> {
        let mut targets: Vec<Target> = self
            .inner
            .read()
            .expect("state lock poisoned")
            .targets
            .values()
            .filter(|t| keep(t))
            .cloned()
            .collect();
        targets.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.id.cmp(&b.id)));
        targets
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
    use crate::core::target::{Source, Target, TargetKind};
    use chrono::Utc;

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

    fn failed(error: &str) -> Status {
        Status::Failed {
            error: error.into(),
            checked_at: Utc::now(),
        }
    }

    #[test]
    fn rediscovery_keeps_probe_status() {
        let state = AppState::new();
        state.upsert_discovered(make_target("m1", TargetKind::Model));
        state.set_status("m1", failed("connection refused"));

        let mut rediscovered = make_target("m1", TargetKind::Model);
        rediscovered.metadata.insert("app".into(), "vllm".into());
        state.upsert_discovered(rediscovered);

        let got = state.get("m1").unwrap();
        assert!(
            matches!(got.status, Status::Failed { ref error, .. } if error == "connection refused")
        );
        assert_eq!(got.metadata.get("app").map(String::as_str), Some("vllm"));
    }

    #[test]
    fn set_status_ignores_removed_target() {
        let state = AppState::new();
        state.set_status("gone", failed("timeout"));
        assert!(state.get("gone").is_none());
    }

    #[test]
    fn all_is_sorted_by_name() {
        let state = AppState::new();
        for id in ["c", "a", "b"] {
            state.upsert(make_target(id, TargetKind::Model));
        }
        let names: Vec<String> = state.all().into_iter().map(|t| t.name).collect();
        assert_eq!(names, ["a", "b", "c"]);
    }
}
