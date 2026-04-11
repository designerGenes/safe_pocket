use anyhow::{Context, Result};
use colored::Colorize;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Config {
    pub aliases: HashMap<String, String>,
}

impl Config {
    pub fn registry_dir() -> Result<PathBuf> {
        let home = dirs::home_dir().context("Failed to get home directory")?;
        let dir = home.join(".safe_pocket").join("registry");
        fs::create_dir_all(&dir).context("Failed to create registry directory")?;
        Ok(dir)
    }

    pub fn config_path() -> Result<PathBuf> {
        Ok(Self::registry_dir()?.join("aliases.json"))
    }

    fn legacy_config_path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("spocket").join("config.json"))
    }

    pub fn load() -> Result<Self> {
        let new_path = Self::config_path()?;

        if new_path.exists() {
            let content = fs::read_to_string(&new_path).context("Failed to read config file")?;
            return Ok(serde_json::from_str(&content).context("Failed to parse config file")?);
        }

        if let Some(old_path) = Self::legacy_config_path() {
            if old_path.exists() {
                let content =
                    fs::read_to_string(&old_path).context("Failed to read legacy config file")?;
                let config: Config =
                    serde_json::from_str(&content).context("Failed to parse legacy config file")?;
                config.save()?;
                let _ = fs::remove_file(&old_path);
                println!(
                    "{} {}",
                    "Migrated alias registry to:".bright_green(),
                    new_path.display().to_string().dimmed()
                );
                return Ok(config);
            }
        }

        Ok(Config::default())
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;
        let content = serde_json::to_string_pretty(self).context("Failed to serialize config")?;

        fs::write(&path, content).context("Failed to write config file")?;

        Ok(())
    }

    pub fn register_alias(&mut self, name: String, path: String) -> Result<()> {
        self.aliases.insert(name, path);
        self.save()
    }

    pub fn unregister_alias(&mut self, name: &str) -> Result<bool> {
        let removed = self.aliases.remove(name).is_some();
        self.save()?;
        Ok(removed)
    }

    pub fn resolve_path(&self, path: &str) -> Result<PathBuf> {
        // Check if it's an alias
        if let Some(aliased_path) = self.aliases.get(path) {
            return Ok(PathBuf::from(aliased_path));
        }

        // Otherwise, expand and canonicalize the path
        let expanded = shellexpand::full(path).context("Failed to expand path")?;

        let path_buf = PathBuf::from(expanded.as_ref());

        // Canonicalize to get absolute path
        if path_buf.exists() {
            path_buf
                .canonicalize()
                .context("Failed to canonicalize path")
        } else {
            Ok(path_buf)
        }
    }
}
