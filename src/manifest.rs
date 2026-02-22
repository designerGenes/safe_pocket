use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

const MANIFEST_FILE: &str = "manifest.json";
const MANIFEST_TMP: &str = "manifest.json.tmp";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub hash: String,
    pub core_paths: Vec<PathBuf>,
    pub created_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_hash: Option<String>,
    #[serde(default)]
    pub children: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub augmented_from: Option<String>,
    #[serde(default = "default_version")]
    pub version: u32,
}

fn default_version() -> u32 {
    1
}

impl Manifest {
    /// Create a fresh manifest for a new pocket.
    pub fn new(hash: String, core_paths: Vec<PathBuf>) -> Self {
        Manifest {
            hash,
            core_paths,
            created_at: Utc::now(),
            parent_hash: None,
            children: Vec::new(),
            augmented_from: None,
            version: 1,
        }
    }

    /// Create a manifest for a cloned pocket (tracks parent lineage).
    pub fn new_cloned(hash: String, core_paths: Vec<PathBuf>, parent_hash: String) -> Self {
        Manifest {
            hash,
            core_paths,
            created_at: Utc::now(),
            parent_hash: Some(parent_hash),
            children: Vec::new(),
            augmented_from: None,
            version: 1,
        }
    }

    /// Create a manifest for a pocket that was migrated via augment/drift-accept.
    /// Preserves the original creation timestamp.
    pub fn new_augmented(
        hash: String,
        core_paths: Vec<PathBuf>,
        augmented_from: String,
        original_created_at: DateTime<Utc>,
    ) -> Self {
        Manifest {
            hash,
            core_paths,
            created_at: original_created_at,
            parent_hash: None,
            children: Vec::new(),
            augmented_from: Some(augmented_from),
            version: 1,
        }
    }

    /// Load manifest from a pocket directory. Returns None if no manifest exists (backwards compat).
    pub fn load(pocket_dir: &Path) -> Result<Option<Self>> {
        let manifest_path = pocket_dir.join(MANIFEST_FILE);

        if !manifest_path.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(&manifest_path)
            .context("Failed to read manifest file")?;

        let manifest: Manifest = serde_json::from_str(&content)
            .context("Failed to parse manifest file")?;

        Ok(Some(manifest))
    }

    /// Atomic write: write to tmp file then rename.
    pub fn save(&self, pocket_dir: &Path) -> Result<()> {
        let manifest_path = pocket_dir.join(MANIFEST_FILE);
        let tmp_path = pocket_dir.join(MANIFEST_TMP);

        let content = serde_json::to_string_pretty(self)
            .context("Failed to serialize manifest")?;

        fs::write(&tmp_path, &content)
            .context("Failed to write manifest tmp file")?;

        fs::rename(&tmp_path, &manifest_path)
            .context("Failed to rename manifest tmp to final")?;

        Ok(())
    }

    /// Add a child hash (idempotent).
    pub fn add_child(&mut self, child_hash: String) {
        if !self.children.contains(&child_hash) {
            self.children.push(child_hash);
        }
    }

    /// Backfill a manifest for an existing pocket that has no manifest.
    /// Recovers metadata from the workspace file (for paths) and dir mtime (for timestamp).
    pub fn backfill(pocket_dir: &Path, hash: &str) -> Result<Self> {
        // Try to read core_paths from the workspace file
        let workspace_file = pocket_dir.join(format!("{}.code-workspace", hash));
        let core_paths = if workspace_file.exists() {
            let content = fs::read_to_string(&workspace_file)
                .context("Failed to read workspace file for backfill")?;

            let ws: serde_json::Value = serde_json::from_str(&content)
                .context("Failed to parse workspace file for backfill")?;

            if let Some(folders) = ws.get("folders").and_then(|f| f.as_array()) {
                folders
                    .iter()
                    .filter_map(|f| f.get("path").and_then(|p| p.as_str()))
                    .map(PathBuf::from)
                    .filter(|p| !p.starts_with(pocket_dir))
                    .collect()
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        // Use directory mtime as created_at
        let created_at = if let Ok(metadata) = fs::metadata(pocket_dir) {
            if let Ok(modified) = metadata.modified() {
                DateTime::<Utc>::from(modified)
            } else {
                Utc::now()
            }
        } else {
            Utc::now()
        };

        let manifest = Manifest {
            hash: hash.to_string(),
            core_paths,
            created_at,
            parent_hash: None,
            children: Vec::new(),
            augmented_from: None,
            version: 1,
        };

        manifest.save(pocket_dir)?;

        Ok(manifest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_manifest_new() {
        let paths = vec![PathBuf::from("/test/a"), PathBuf::from("/test/b")];
        let m = Manifest::new("abc123".to_string(), paths.clone());

        assert_eq!(m.hash, "abc123");
        assert_eq!(m.core_paths, paths);
        assert!(m.parent_hash.is_none());
        assert!(m.children.is_empty());
        assert!(m.augmented_from.is_none());
        assert_eq!(m.version, 1);
    }

    #[test]
    fn test_manifest_new_cloned() {
        let paths = vec![PathBuf::from("/test/a")];
        let m = Manifest::new_cloned("child123".to_string(), paths, "parent456".to_string());

        assert_eq!(m.parent_hash, Some("parent456".to_string()));
    }

    #[test]
    fn test_manifest_add_child_idempotent() {
        let mut m = Manifest::new("abc".to_string(), vec![]);
        m.add_child("child1".to_string());
        m.add_child("child1".to_string());
        m.add_child("child2".to_string());

        assert_eq!(m.children.len(), 2);
        assert_eq!(m.children, vec!["child1", "child2"]);
    }

    #[test]
    fn test_manifest_save_and_load() {
        let tmp = std::env::temp_dir().join("spocket_manifest_test");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();

        let paths = vec![PathBuf::from("/test/a"), PathBuf::from("/test/b")];
        let m = Manifest::new("testhash".to_string(), paths.clone());
        m.save(&tmp).unwrap();

        let loaded = Manifest::load(&tmp).unwrap().unwrap();
        assert_eq!(loaded.hash, "testhash");
        assert_eq!(loaded.core_paths, paths);
        assert_eq!(loaded.version, 1);

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_manifest_load_missing() {
        let tmp = std::env::temp_dir().join("spocket_manifest_missing");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();

        let loaded = Manifest::load(&tmp).unwrap();
        assert!(loaded.is_none());

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_manifest_serialization_roundtrip() {
        let paths = vec![PathBuf::from("/a"), PathBuf::from("/b")];
        let mut m = Manifest::new_cloned("h1".to_string(), paths, "h0".to_string());
        m.add_child("h2".to_string());
        m.augmented_from = Some("h_old".to_string());

        let json = serde_json::to_string_pretty(&m).unwrap();
        let deserialized: Manifest = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.hash, "h1");
        assert_eq!(deserialized.parent_hash, Some("h0".to_string()));
        assert_eq!(deserialized.children, vec!["h2"]);
        assert_eq!(deserialized.augmented_from, Some("h_old".to_string()));
    }
}
