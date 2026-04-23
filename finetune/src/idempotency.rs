//! Idempotency-key store for Layer B job starts.
//!
//! Track: A | Feature: 001-job-based-cli-api
//!
//! Contract (§2.4, §2.5 of the redesign):
//!   1. Same `(workflow_id, idempotency_key)` + **same payload hash** →
//!      return the **same** `job_id`. No new job row.
//!   2. Same key + **different** payload → `409 CONFLICT`.
//!   3. Absent key → server MUST generate one and echo it back.
//!
//! This module provides the purely-in-memory trait + a default impl. The
//! gateway wraps it with a SQLite-backed impl so the contract survives
//! restarts (the in-memory version is test-only).

use std::collections::HashMap;
use std::sync::Mutex;

use crate::job_error::JobError;

/// Outcome of `check_or_insert` — tells the caller whether to create a new
/// job or echo the previously-created one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdempotencyOutcome {
    /// First time we've seen this key. Caller MUST create the job now and
    /// later call `record_job_id(...)` to finalize the mapping.
    Fresh,
    /// Same key + same payload seen before. Return the remembered job_id.
    Replay { job_id: String },
}

/// The subset of a request we hash to decide "same payload". Opaque bytes
/// — callers typically use `canonical_payload_hash()` below. 8 bytes is
/// enough for user-scoped dedup (collisions only cause a false-409 for
/// the same user, not a security issue).
pub type PayloadHash = [u8; 8];

/// Abstract store so production code can swap the in-memory impl for a
/// SQLite-backed one without changing handler logic.
pub trait IdempotencyStore: Send + Sync {
    fn check_or_insert(
        &self,
        workflow_id: &str,
        key: &str,
        payload_hash: PayloadHash,
    ) -> Result<IdempotencyOutcome, JobError>;

    fn record_job_id(&self, workflow_id: &str, key: &str, job_id: &str);
}

/// In-memory idempotency store. Thread-safe via `Mutex`; not durable across
/// process restarts. Used by unit tests and as the skeleton the SQLite impl
/// mirrors.
#[derive(Default)]
pub struct InMemoryIdempotencyStore {
    inner: Mutex<HashMap<(String, String), Entry>>,
}

#[derive(Clone)]
struct Entry {
    payload_hash: PayloadHash,
    job_id: Option<String>,
}

impl InMemoryIdempotencyStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl IdempotencyStore for InMemoryIdempotencyStore {
    fn check_or_insert(
        &self,
        workflow_id: &str,
        key: &str,
        payload_hash: PayloadHash,
    ) -> Result<IdempotencyOutcome, JobError> {
        let mut guard = self.inner.lock().expect("idempotency store poisoned");
        let composite = (workflow_id.to_string(), key.to_string());
        match guard.get(&composite) {
            Some(existing) if existing.payload_hash == payload_hash => {
                // Same key + same payload. Replay if we already recorded a
                // job_id; otherwise tell the caller it's fresh (they still
                // own the create step — we just reserve the slot).
                match &existing.job_id {
                    Some(id) => Ok(IdempotencyOutcome::Replay { job_id: id.clone() }),
                    None => Ok(IdempotencyOutcome::Fresh),
                }
            }
            Some(_) => Err(JobError::conflict(format!(
                "idempotency_key `{key}` was previously used with a different payload"
            ))),
            None => {
                guard.insert(
                    composite,
                    Entry {
                        payload_hash,
                        job_id: None,
                    },
                );
                Ok(IdempotencyOutcome::Fresh)
            }
        }
    }

    fn record_job_id(&self, workflow_id: &str, key: &str, job_id: &str) {
        let mut guard = self.inner.lock().expect("idempotency store poisoned");
        if let Some(entry) = guard.get_mut(&(workflow_id.to_string(), key.to_string())) {
            entry.job_id = Some(job_id.to_string());
        }
    }
}

/// Canonical payload hash helper. Hashes the canonicalised JSON (object
/// keys sorted, so `{a:1,b:2}` and `{b:2,a:1}` hash the same). Uses
/// `DefaultHasher` — non-crypto; sufficient for scoped dedup, and keeps
/// the crate dep-free (no `sha2`). If cross-process stability ever matters,
/// swap to a stable hash like xxhash via a feature flag.
pub fn canonical_payload_hash(payload: &serde_json::Value) -> PayloadHash {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let canonical = serde_json::to_string(&canonicalise(payload.clone()))
        .expect("json canonicalisation");
    let mut h = DefaultHasher::new();
    canonical.hash(&mut h);
    h.finish().to_be_bytes()
}

fn canonicalise(v: serde_json::Value) -> serde_json::Value {
    match v {
        serde_json::Value::Object(map) => {
            let sorted: std::collections::BTreeMap<String, serde_json::Value> =
                map.into_iter().map(|(k, v)| (k, canonicalise(v))).collect();
            serde_json::to_value(sorted).unwrap()
        }
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.into_iter().map(canonicalise).collect())
        }
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn hash(v: serde_json::Value) -> PayloadHash {
        canonical_payload_hash(&v)
    }

    #[test]
    fn canonicalisation_ignores_key_order() {
        let a = hash(json!({"a": 1, "b": 2}));
        let b = hash(json!({"b": 2, "a": 1}));
        assert_eq!(a, b);
    }

    #[test]
    fn first_call_with_a_key_is_fresh_and_records_the_job_id() {
        let store = InMemoryIdempotencyStore::new();
        let outcome = store
            .check_or_insert("wf-1", "key-1", hash(json!({"operation": "eval-run"})))
            .unwrap();
        assert_eq!(outcome, IdempotencyOutcome::Fresh);

        store.record_job_id("wf-1", "key-1", "job-abc");

        // Replay with the same key + same payload returns the recorded job_id.
        let replay = store
            .check_or_insert("wf-1", "key-1", hash(json!({"operation": "eval-run"})))
            .unwrap();
        assert_eq!(
            replay,
            IdempotencyOutcome::Replay {
                job_id: "job-abc".into()
            }
        );
    }

    #[test]
    fn same_key_different_payload_yields_conflict() {
        let store = InMemoryIdempotencyStore::new();
        store
            .check_or_insert("wf-1", "key-2", hash(json!({"operation": "eval-run"})))
            .unwrap();
        store.record_job_id("wf-1", "key-2", "job-xyz");

        let err = store
            .check_or_insert("wf-1", "key-2", hash(json!({"operation": "train-run"})))
            .expect_err("payload collision must 409");
        assert_eq!(err.code, crate::job_error::JobErrorCode::Conflict);
    }

    #[test]
    fn keys_are_scoped_per_workflow() {
        // Same key under two different workflows is two distinct slots.
        let store = InMemoryIdempotencyStore::new();
        store
            .check_or_insert("wf-a", "shared-key", hash(json!({"op": "a"})))
            .unwrap();
        let outcome = store
            .check_or_insert("wf-b", "shared-key", hash(json!({"op": "a"})))
            .unwrap();
        assert_eq!(outcome, IdempotencyOutcome::Fresh);
    }
}
