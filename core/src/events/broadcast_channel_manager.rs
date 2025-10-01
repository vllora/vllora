use crate::telemetry::ProjectTraceMap;
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::info;

/// Manages broadcast channels with automatic cleanup to prevent memory leaks.
///
/// This struct provides a wrapper around ProjectTraceMap to ensure that broadcast
/// channels are properly cleaned up when there are no more active receivers.
///
/// ## Memory Leak Prevention
///
/// The original implementation had a memory leak where broadcast channels were created
/// but never removed from the map, even when all receivers disconnected. This led to
/// gradual memory accumulation over time.
///
/// This implementation addresses the issue by:
/// 1. Tracking when clients disconnect and cleaning up empty channels
/// 2. Running a periodic cleanup task to catch any edge cases
/// 3. Checking receiver counts before operations to ensure channels are still needed
#[derive(Clone)]
pub struct BroadcastChannelManager {
    map: Arc<ProjectTraceMap>,
}

impl BroadcastChannelManager {
    /// Creates a new BroadcastChannelManager with the given ProjectTraceMap
    pub fn new(map: Arc<ProjectTraceMap>) -> Self {
        Self { map }
    }

    /// Gets or creates a broadcast channel for the given project ID
    /// Returns the broadcast sender
    pub fn get_or_create_channel(
        &self,
        project_id: &str,
    ) -> Result<broadcast::Sender<crate::telemetry::Span>, String> {
        // Check if channel exists and has receivers
        if let Some(entry) = self.map.get(project_id) {
            let sender = entry.value();
            if sender.receiver_count() > 0 {
                return Ok(sender.clone());
            }
            // If no receivers, we'll create a new channel below
        }

        // Create new channel
        let (sender, receiver) = broadcast::channel(128);
        drop(receiver); // Drop the initial receiver to avoid leaks
        self.map.insert(project_id.to_string(), sender.clone());
        Ok(sender)
    }

    /// Attempts to clean up a channel if it has no receivers
    pub fn try_cleanup_channel(&self, project_id: &str) {
        // Scope the entry so it is dropped before remove
        let should_remove = {
            if let Some(entry) = self.map.get(project_id) {
                let sender = entry.value().clone();
                sender.receiver_count() == 0
            } else {
                false
            }
        };
        if should_remove {
            self.map.remove(project_id);
        }
    }

    /// Runs a full cleanup of all channels with no receivers
    pub fn cleanup_all_empty_channels(&self) -> usize {
        let mut channels_removed = 0;
        let mut channels_to_remove = Vec::new();

        // First pass: identify channels to remove
        for entry in self.map.iter() {
            let project_id = entry.key().clone();
            let sender = entry.value();

            if sender.receiver_count() == 0 {
                channels_to_remove.push(project_id);
            }
        }

        // Second pass: remove identified channels
        for project_id in channels_to_remove {
            self.map.remove(&project_id);
            channels_removed += 1;
        }

        channels_removed
    }

    /// Gets the underlying ProjectTraceMap
    pub fn inner(&self) -> &Arc<ProjectTraceMap> {
        &self.map
    }
}

/// Starts a background task that periodically cleans up empty broadcast channels
pub fn start_cleanup_task(manager: BroadcastChannelManager) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(60)); // Every 1 minute

        loop {
            interval.tick().await;

            let channels_removed = manager.cleanup_all_empty_channels();
            if channels_removed > 0 {
                info!(
                    "Periodic cleanup removed {} empty broadcast channels",
                    channels_removed
                );
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_manager() -> BroadcastChannelManager {
        BroadcastChannelManager::new(Default::default())
    }

    #[tokio::test]
    async fn test_receiver_count_increases_and_decreases() {
        let manager = make_manager();
        let project_id = "test_project";
        let sender = manager.get_or_create_channel(project_id).unwrap();
        assert_eq!(sender.receiver_count(), 0);
        let receiver1 = sender.subscribe();
        assert_eq!(sender.receiver_count(), 1);
        {
            let _receiver2 = sender.subscribe();
            assert_eq!(sender.receiver_count(), 2);
        }
        // _receiver2 dropped
        assert_eq!(sender.receiver_count(), 1);
        drop(receiver1);
        assert_eq!(sender.receiver_count(), 0);
    }

    #[tokio::test]
    async fn test_try_cleanup_channel_removes_empty() {
        let manager = make_manager();
        let project_id = "test_project";
        let sender = manager.get_or_create_channel(project_id).unwrap();
        {
            let receiver = sender.subscribe();
            assert!(manager.inner().contains_key(project_id));
            println!("Before drop: receiver_count = {}", sender.receiver_count());
            drop(receiver); // Explicitly drop inside a block
            println!("After drop: receiver_count = {}", sender.receiver_count());
        }
        println!("Calling try_cleanup_channel...");
        manager.try_cleanup_channel(project_id);
        println!(
            "After cleanup: contains_key = {}",
            manager.inner().contains_key(project_id)
        );
        assert!(!manager.inner().contains_key(project_id));
    }

    #[tokio::test]
    async fn test_cleanup_all_empty_channels() {
        let manager = make_manager();
        let p1 = "p1";
        let p2 = "p2";
        let s1 = manager.get_or_create_channel(p1).unwrap();
        let s2 = manager.get_or_create_channel(p2).unwrap();
        let r1 = s1.subscribe();
        let r2 = s2.subscribe();
        assert!(manager.inner().contains_key(p1));
        assert!(manager.inner().contains_key(p2));
        drop(r1);
        drop(r2);
        let removed = manager.cleanup_all_empty_channels();
        assert_eq!(removed, 2);
        assert!(!manager.inner().contains_key(p1));
        assert!(!manager.inner().contains_key(p2));
    }

    #[tokio::test]
    async fn test_channel_not_removed_if_receiver_exists() {
        let manager = make_manager();
        let project_id = "test_project";
        let sender = manager.get_or_create_channel(project_id).unwrap();
        let _receiver = sender.subscribe();
        manager.try_cleanup_channel(project_id);
        assert!(manager.inner().contains_key(project_id));
    }
}
