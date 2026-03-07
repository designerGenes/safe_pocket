use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Config {
    pub aliases: HashMap<String, String>,
}

impl Config {
    pub fn config_dir() -> Result<PathBuf> {
        let config_dir = dirs::config_dir()
            .context("Failed to get config directory")?
            .join("spocket");

        fs::create_dir_all(&config_dir).context("Failed to create config directory")?;

        Ok(config_dir)
    }

    pub fn config_path() -> Result<PathBuf> {
        Ok(Self::config_dir()?.join("config.json"))
    }

    pub fn load() -> Result<Self> {
        let path = Self::config_path()?;

        if !path.exists() {
            return Ok(Config::default());
        }

        let content = fs::read_to_string(&path).context("Failed to read config file")?;

        let config: Config =
            serde_json::from_str(&content).context("Failed to parse config file")?;

        Ok(config)
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
