use chrono::{DateTime, Local, NaiveDate};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

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
}

impl SnipeQueue {
    /// Load the snipe queue from file, or create empty if doesn't exist
    pub fn load() -> Result<Self> {
        if !Path::new(SNIPES_FILE).exists() {
            return Ok(Self::default());
        }

        let content = fs::read_to_string(SNIPES_FILE).map_err(|e| {
            GymSniperError::Config(format!("Failed to read snipes file: {}", e))
        })?;

        let queue: SnipeQueue = serde_json::from_str(&content).map_err(|e| {
            GymSniperError::Config(format!("Failed to parse snipes file: {}", e))
        })?;

        Ok(queue)
    }

    /// Save the snipe queue to file
    pub fn save(&self) -> Result<()> {
        let content = serde_json::to_string_pretty(self).map_err(|e| {
            GymSniperError::Config(format!("Failed to serialize snipes: {}", e))
        })?;

        fs::write(SNIPES_FILE, content).map_err(|e| {
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

    /// Mark a snipe as completed
    pub fn mark_completed(&mut self, class_id: u64) -> Result<()> {
        if let Some(entry) = self.snipes.iter_mut().find(|s| s.class_id == class_id) {
            entry.status = SnipeStatus::Completed;
            self.save()?;
        }
        Ok(())
    }

    /// Mark a snipe as failed with error message
    pub fn mark_failed(&mut self, class_id: u64, error: &str) -> Result<()> {
        if let Some(entry) = self.snipes.iter_mut().find(|s| s.class_id == class_id) {
            entry.status = SnipeStatus::Failed;
            entry.error_message = Some(error.to_string());
            self.save()?;
        }
        Ok(())
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
