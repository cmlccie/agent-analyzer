use crate::core::target::{Status, Target, TargetKind};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tokio::sync::Notify;

/// Shared, mutable application state. Owned by the `serve` process and read
/// by HTTP handlers, discovery tasks, and probe tasks.
#[derive(Debug, Default, Clone)]
pub struct AppState {
    inner: Arc<RwLock<StateInner>>,
    refresh: Arc<Notify>,
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

    /// Replace the target set with the result of a full discovery pass.
    ///
    /// Targets that were not discovered again are dropped, so deleted Services and removed config
    /// entries disappear. Targets that were keep their probe status, `since` and history:
    /// discovery always yields `Status::Unknown`, and letting that overwrite would flip a probed
    /// target back to `?` and lose its last error.
    pub fn reconcile(&self, discovered: Vec<Target>) {
        let mut inner = self.inner.write().expect("state lock poisoned");
        let mut previous = std::mem::take(&mut inner.targets);
        inner.targets = discovered
            .into_iter()
            .map(|mut t| {
                if let Some(old) = previous.remove(&t.id) {
                    t.status = old.status;
                    t.since = old.since;
                    t.history = old.history;
                }
                (t.id.clone(), t)
            })
            .collect();
    }

    /// Record a probe result for a target. Only probe-owned fields change, so metadata refreshed
    /// by a concurrent discovery pass is kept, and a target removed while its probe was in flight
    /// stays removed.
    pub fn record(&self, id: &str, status: Status) {
        if let Some(t) = self
            .inner
            .write()
            .expect("state lock poisoned")
            .targets
            .get_mut(id)
        {
            t.record(status);
        }
    }

    /// Ask the background loop to run discovery and a probe pass now. A request made while a
    /// pass is running is kept and served when it finishes.
    pub fn request_refresh(&self) {
        self.refresh.notify_one();
    }

    /// Wait for the next `request_refresh`. Intended for a single consumer.
    pub async fn refresh_requested(&self) {
        self.refresh.notified().await;
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
            since: None,
            history: Vec::new(),
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
            kind: Default::default(),
            error: error.into(),
            checked_at: Utc::now(),
        }
    }

    #[test]
    fn reconcile_keeps_probe_state_and_prunes_missing() {
        let state = AppState::new();
        state.reconcile(vec![
            make_target("m1", TargetKind::Model),
            make_target("gone", TargetKind::Model),
        ]);
        state.record("m1", failed("connection refused"));

        let mut rediscovered = make_target("m1", TargetKind::Model);
        rediscovered.metadata.insert("app".into(), "vllm".into());
        state.reconcile(vec![rediscovered]);

        let got = state.get("m1").unwrap();
        assert!(
            matches!(got.status, Status::Failed { ref error, .. } if error == "connection refused")
        );
        assert_eq!(got.history, [false]);
        assert!(got.since.is_some());
        assert_eq!(got.metadata.get("app").map(String::as_str), Some("vllm"));
        assert!(state.get("gone").is_none());
    }

    #[test]
    fn record_ignores_removed_target() {
        let state = AppState::new();
        state.record("gone", failed("timeout"));
        assert!(state.get("gone").is_none());
    }

    #[tokio::test]
    async fn refresh_request_is_not_lost_without_a_waiter() {
        let state = AppState::new();
        state.request_refresh();
        tokio::time::timeout(std::time::Duration::from_secs(1), state.refresh_requested())
            .await
            .expect("stored refresh request should complete immediately");
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
