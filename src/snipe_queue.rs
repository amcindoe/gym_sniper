use chrono::{DateTime, Local, NaiveDate};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{GymSniperError, Result};

const SNIPES_FILE: &str = "snipes.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnipeEntry {
    pub class_id: u64,
    pub class_name: String,
    pub class_time: DateTime<Local>,
    pub booking_window: DateTime<Local>,
    pub trainer: Option<String>,
    pub added_at: DateTime<Local>,
    pub status: SnipeStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum SnipeStatus {
    Pending,
    Completed,
    Failed,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct SnipeQueue {
    pub snipes: Vec<SnipeEntry>,
    #[serde(skip)]
    file_path: Option<PathBuf>,
}

impl SnipeQueue {
    /// Load the snipe queue from file, or create empty if doesn't exist
    pub fn load() -> Result<Self> {
        Self::load_from(Path::new(SNIPES_FILE))
    }

    /// Load the snipe queue from a specific path
    pub fn load_from(path: &Path) -> Result<Self> {
        if !path.exists() {
            let mut queue = Self::default();
            queue.file_path = Some(path.to_path_buf());
            return Ok(queue);
        }

        let content = fs::read_to_string(path).map_err(|e| {
            GymSniperError::Config(format!("Failed to read snipes file: {}", e))
        })?;

        let mut queue: SnipeQueue = serde_json::from_str(&content).map_err(|e| {
            GymSniperError::Config(format!("Failed to parse snipes file: {}", e))
        })?;
        queue.file_path = Some(path.to_path_buf());

        Ok(queue)
    }

    /// Save the snipe queue to file
    pub fn save(&self) -> Result<()> {
        let path = self.file_path.as_deref().unwrap_or(Path::new(SNIPES_FILE));
        let content = serde_json::to_string_pretty(self).map_err(|e| {
            GymSniperError::Config(format!("Failed to serialize snipes: {}", e))
        })?;

        fs::write(path, content).map_err(|e| {
            GymSniperError::Config(format!("Failed to write snipes file: {}", e))
        })?;

        Ok(())
    }

    /// Check if there's already a snipe for the given date
    pub fn has_snipe_for_date(&self, date: NaiveDate) -> Option<&SnipeEntry> {
        self.snipes.iter().find(|s| {
            s.status == SnipeStatus::Pending && s.class_time.date_naive() == date
        })
    }

    /// Add a new snipe entry
    pub fn add(&mut self, entry: SnipeEntry) -> Result<()> {
        let class_date = entry.class_time.date_naive();

        // Check if there's already a pending snipe for this date
        if let Some(existing) = self.has_snipe_for_date(class_date) {
            return Err(GymSniperError::Config(format!(
                "Already have a snipe queued for {}: {} at {} (class ID {}). Only one class per day allowed.",
                class_date.format("%a %d %b"),
                existing.class_name,
                existing.class_time.format("%H:%M"),
                existing.class_id
            )));
        }

        // Check if this class is already in the queue
        if self.snipes.iter().any(|s| s.class_id == entry.class_id) {
            return Err(GymSniperError::Config(format!(
                "Class {} is already in the snipe queue",
                entry.class_id
            )));
        }

        self.snipes.push(entry);
        self.save()?;
        Ok(())
    }

    /// Remove a snipe by class ID
    pub fn remove(&mut self, class_id: u64) -> Result<bool> {
        let initial_len = self.snipes.len();
        self.snipes.retain(|s| s.class_id != class_id);

        if self.snipes.len() < initial_len {
            self.save()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Get all pending snipes sorted by booking window time
    pub fn pending_snipes(&self) -> Vec<&SnipeEntry> {
        let mut pending: Vec<_> = self.snipes.iter()
            .filter(|s| s.status == SnipeStatus::Pending)
            .collect();
        pending.sort_by_key(|s| s.booking_window);
        pending
    }

    /// Clean up old completed/failed entries (older than 7 days)
    pub fn cleanup_old_entries(&mut self) -> Result<()> {
        let cutoff = Local::now() - chrono::Duration::days(7);
        let initial_len = self.snipes.len();

        self.snipes.retain(|s| {
            s.status == SnipeStatus::Pending || s.class_time > cutoff
        });

        if self.snipes.len() < initial_len {
            self.save()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use tempfile::TempDir;

    fn make_entry(class_id: u64, name: &str, days_from_now: i64, status: SnipeStatus) -> SnipeEntry {
        let class_time = Local::now() + Duration::days(days_from_now);
        let booking_window = class_time - Duration::days(7) - Duration::hours(2);
        SnipeEntry {
            class_id,
            class_name: name.to_string(),
            class_time,
            booking_window,
            trainer: None,
            added_at: Local::now(),
            status,
            error_message: None,
        }
    }

    fn test_queue(dir: &TempDir) -> SnipeQueue {
        let path = dir.path().join("snipes.json");
        SnipeQueue::load_from(&path).unwrap()
    }

    #[test]
    fn add_succeeds() {
        let dir = TempDir::new().unwrap();
        let mut queue = test_queue(&dir);
        let entry = make_entry(100, "Yoga", 8, SnipeStatus::Pending);
        queue.add(entry).unwrap();
        assert_eq!(queue.snipes.len(), 1);
        assert_eq!(queue.snipes[0].class_id, 100);
    }

    #[test]
    fn add_rejects_duplicate_class_id() {
        let dir = TempDir::new().unwrap();
        let mut queue = test_queue(&dir);
        queue.add(make_entry(100, "Yoga", 8, SnipeStatus::Pending)).unwrap();
        let result = queue.add(make_entry(100, "Spin", 9, SnipeStatus::Pending));
        assert!(result.is_err());
    }

    #[test]
    fn add_rejects_same_date_conflict() {
        let dir = TempDir::new().unwrap();
        let mut queue = test_queue(&dir);
        // Add two entries for the same day (both 8 days from now)
        queue.add(make_entry(100, "Yoga", 8, SnipeStatus::Pending)).unwrap();
        let result = queue.add(make_entry(200, "Spin", 8, SnipeStatus::Pending));
        assert!(result.is_err());
    }

    #[test]
    fn remove_returns_true_when_found() {
        let dir = TempDir::new().unwrap();
        let mut queue = test_queue(&dir);
        queue.add(make_entry(100, "Yoga", 8, SnipeStatus::Pending)).unwrap();
        assert!(queue.remove(100).unwrap());
        assert!(queue.snipes.is_empty());
    }

    #[test]
    fn remove_returns_false_when_not_found() {
        let dir = TempDir::new().unwrap();
        let mut queue = test_queue(&dir);
        assert!(!queue.remove(999).unwrap());
    }

    #[test]
    fn pending_snipes_filters_and_sorts() {
        let dir = TempDir::new().unwrap();
        let mut queue = test_queue(&dir);

        // Add entries with different statuses and times
        // Completed entry (should be filtered out)
        queue.snipes.push(make_entry(1, "Done", 5, SnipeStatus::Completed));
        // Pending entries (further in future first, nearer second)
        queue.snipes.push(make_entry(2, "Later", 10, SnipeStatus::Pending));
        queue.snipes.push(make_entry(3, "Sooner", 8, SnipeStatus::Pending));

        let pending = queue.pending_snipes();
        assert_eq!(pending.len(), 2);
        // Should be sorted by booking_window (sooner first)
        assert_eq!(pending[0].class_id, 3);
        assert_eq!(pending[1].class_id, 2);
    }

    #[test]
    fn cleanup_old_entries_removes_old_non_pending() {
        let dir = TempDir::new().unwrap();
        let mut queue = test_queue(&dir);

        // Old completed entry (class_time 10 days ago)
        queue.snipes.push(make_entry(1, "Old", -10, SnipeStatus::Completed));
        // Recent completed entry (class_time 3 days ago - within 7 day cutoff)
        queue.snipes.push(make_entry(2, "Recent", -3, SnipeStatus::Failed));
        // Old but still pending (should be kept)
        queue.snipes.push(make_entry(3, "Pending", -10, SnipeStatus::Pending));
        queue.save().unwrap();

        queue.cleanup_old_entries().unwrap();

        assert_eq!(queue.snipes.len(), 2);
        let ids: Vec<u64> = queue.snipes.iter().map(|s| s.class_id).collect();
        assert!(ids.contains(&2)); // recent failed kept
        assert!(ids.contains(&3)); // pending always kept
        assert!(!ids.contains(&1)); // old completed removed
    }

    #[test]
    fn load_and_save_roundtrip() {
        let dir = TempDir::new().unwrap();
        let mut queue = test_queue(&dir);
        queue.add(make_entry(42, "Yoga Flow", 8, SnipeStatus::Pending)).unwrap();

        // Load from same path
        let path = dir.path().join("snipes.json");
        let loaded = SnipeQueue::load_from(&path).unwrap();
        assert_eq!(loaded.snipes.len(), 1);
        assert_eq!(loaded.snipes[0].class_id, 42);
        assert_eq!(loaded.snipes[0].class_name, "Yoga Flow");
    }
}
