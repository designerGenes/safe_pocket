use anyhow::{anyhow, Context, Result};
use colored::Colorize;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::hash::hash_paths;

#[derive(Debug, Serialize, Deserialize)]
pub struct VSCodeWorkspace {
    pub folders: Vec<WorkspaceFolder>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settings: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WorkspaceFolder {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

pub struct Workspace {
    pub hash: String,
    pub core_paths: Vec<PathBuf>,
    pub sidecar_paths: Vec<PathBuf>,
    pub pocket_dir: PathBuf,
    pub create_readmes: bool,
}

impl Workspace {
    pub fn spocket_dir() -> Result<PathBuf> {
        let home = dirs::home_dir()
            .context("Failed to get home directory")?;

        let spocket_dir = home.join(".spocket");

        fs::create_dir_all(&spocket_dir)
            .context("Failed to create .spocket directory")?;

        Ok(spocket_dir)
    }

    pub fn new(core_paths: Vec<PathBuf>, sidecar_paths: Vec<PathBuf>, create_readmes: bool) -> Result<Self> {
        let hash = hash_paths(&core_paths);
        let pocket_dir = Self::spocket_dir()?.join(&hash);

        Ok(Workspace {
            hash,
            core_paths,
            sidecar_paths,
            pocket_dir,
            create_readmes,
        })
    }

    pub fn workspace_file_path(&self) -> PathBuf {
        self.pocket_dir.join(format!("{}.code-workspace", self.hash))
    }

    pub fn exists(&self) -> bool {
        self.pocket_dir.exists() && self.workspace_file_path().exists()
    }

    pub fn create(&self) -> Result<()> {
        if self.exists() {
            println!("{}", "Workspace already exists".dimmed());
            return self.check_mismatch();
        }

        println!("{}", "Creating new safe pocket...".bright_white());

        // Create pocket directory structure
        self.create_pocket_structure()?;

        // Create workspace file
        self.create_workspace_file()?;

        // Initialize git
        self.init_git()?;

        println!("{} {}", "Created workspace:".bright_green(), self.hash.bright_yellow());
        println!("  {} {}", "Location:".dimmed(), self.pocket_dir.display().to_string().bright_blue());

        Ok(())
    }

    fn create_pocket_structure(&self) -> Result<()> {
        fs::create_dir_all(&self.pocket_dir)
            .context("Failed to create pocket directory")?;

        // Create README in root if enabled
        if self.create_readmes {
            let readme = self.pocket_dir.join("README.md");
            fs::write(
                &readme,
                format!("# Safe Pocket: {}\n\nThis is a Safe Pocket workspace directory. It contains:\n\n\
                - `.github/copilot-instructions.md` - Custom AI copilot instructions\n\
                - `.github/prompts/` - Reusable prompt templates\n\
                - `FEATURES/` - Feature ideas and documentation\n\
                - `observations/` - AI-generated insights and learnings\n\n\
                ## Usage\n\n\
                This directory is automatically managed by spocket. Edit the files above to customize \
                your AI assistant's behavior for the workspace directories:\n\n{}\n\n\
                Learn more: https://github.com/your-repo/safe_pocket\n",
                self.hash,
                self.core_paths.iter()
                    .map(|p| format!("- {}", p.display()))
                    .collect::<Vec<_>>()
                    .join("\n")
                )
            )
            .context("Failed to create README.md")?;
        }

        // Create .github/prompts/
        let github_prompts = self.pocket_dir.join(".github").join("prompts");
        fs::create_dir_all(&github_prompts)
            .context("Failed to create .github/prompts directory")?;

        if self.create_readmes {
            let prompts_readme = github_prompts.join("README.md");
            fs::write(
                &prompts_readme,
                "# Prompts Directory\n\n\
                Store reusable prompt templates here.\n\n\
                ## Example\n\n\
                Create a file like `code-review.md` with a prompt template:\n\n\
                ```\n\
                Please review this code for:\n\
                - Security vulnerabilities\n\
                - Performance issues\n\
                - Code style consistency\n\
                ```\n\n\
                Then reference it in your copilot conversations.\n"
            )
            .context("Failed to create prompts README.md")?;
        }

        // Create copilot-instructions.md
        let copilot_instructions = self.pocket_dir.join(".github").join("copilot-instructions.md");
        fs::write(
            &copilot_instructions,
            "# Copilot Instructions\n\nAdd your custom copilot instructions here.\n",
        )
        .context("Failed to create copilot-instructions.md")?;

        // Create FEATURES/00.md
        let features_dir = self.pocket_dir.join("FEATURES");
        fs::create_dir_all(&features_dir)
            .context("Failed to create FEATURES directory")?;

        let features_file = features_dir.join("00.md");
        fs::write(
            &features_file,
            "# Features\n\nAdd your feature ideas and documentation here.\n",
        )
        .context("Failed to create FEATURES/00.md")?;

        // Create observations/
        let observations_dir = self.pocket_dir.join("observations");
        fs::create_dir_all(&observations_dir)
            .context("Failed to create observations directory")?;

        if self.create_readmes {
            let observations_readme = observations_dir.join("README.md");
            fs::write(
                &observations_readme,
                "# Observations Directory\n\n\
                This directory is for AI-generated insights and learnings discovered during your work.\n\n\
                ## Purpose\n\n\
                As you work with your AI copilot, it may discover:\n\
                - Common patterns in your codebase\n\
                - Recurring issues or bugs\n\
                - Useful shortcuts or techniques\n\
                - Project-specific conventions\n\n\
                Document these observations here so they can inform future sessions.\n\n\
                ## Format\n\n\
                Create dated files like `2024-01-15-auth-patterns.md` with your findings.\n"
            )
            .context("Failed to create observations README.md")?;
        }

        Ok(())
    }

    pub fn create_workspace_file(&self) -> Result<()> {
        let mut folders = Vec::new();

        // Add core paths
        for path in &self.core_paths {
            folders.push(WorkspaceFolder {
                path: path.to_string_lossy().to_string(),
                name: None,
            });
        }

        // Add the safe pocket itself
        folders.push(WorkspaceFolder {
            path: self.pocket_dir.to_string_lossy().to_string(),
            name: Some(format!("[Safe Pocket] {}", self.hash)),
        });

        let workspace = VSCodeWorkspace {
            folders,
            settings: None,
        };

        let workspace_json = serde_json::to_string_pretty(&workspace)
            .context("Failed to serialize workspace")?;

        let workspace_path = self.workspace_file_path();
        fs::write(&workspace_path, workspace_json)
            .context("Failed to write workspace file")?;

        Ok(())
    }

    fn init_git(&self) -> Result<()> {
        let output = Command::new("git")
            .args(["init"])
            .current_dir(&self.pocket_dir)
            .output()
            .context("Failed to execute git init")?;

        if !output.status.success() {
            return Err(anyhow!("Git init failed: {}", String::from_utf8_lossy(&output.stderr)));
        }

        Ok(())
    }

    pub fn check_mismatch(&self) -> Result<()> {
        let workspace_path = self.workspace_file_path();

        if !workspace_path.exists() {
            return Ok(());
        }

        let content = fs::read_to_string(&workspace_path)
            .context("Failed to read workspace file")?;

        let workspace: VSCodeWorkspace = serde_json::from_str(&content)
            .context("Failed to parse workspace file")?;

        // Extract paths from workspace (excluding the safe pocket itself)
        let workspace_paths: Vec<PathBuf> = workspace
            .folders
            .iter()
            .map(|f| PathBuf::from(&f.path))
            .filter(|p| !p.starts_with(&self.pocket_dir))
            .collect();

        // Compare with core paths
        let workspace_set: HashSet<_> = workspace_paths.iter().collect();
        let core_set: HashSet<_> = self.core_paths.iter().collect();

        if workspace_set != core_set {
            println!("\n{}", "Warning: Workspace mismatch detected!".bright_yellow());
            println!("  {} Hash is based on: {:?}", "Expected:".dimmed(), self.core_paths);
            println!("  {} Workspace contains: {:?}", "Found:".dimmed(), workspace_paths);
            println!("\n  {} Run workspace file cleanup to fix this", "Tip:".bright_cyan());
        }

        Ok(())
    }

    pub fn open(&self) -> Result<()> {
        let workspace_path = self.workspace_file_path();

        if !workspace_path.exists() {
            return Err(anyhow!("Workspace file does not exist"));
        }

        // If there are sidecars, we need to temporarily add them
        if !self.sidecar_paths.is_empty() {
            println!("Adding {} sidecar directories...",
                self.sidecar_paths.len().to_string().bright_yellow()
            );

            // Read current workspace
            let content = fs::read_to_string(&workspace_path)
                .context("Failed to read workspace file")?;

            let mut workspace: VSCodeWorkspace = serde_json::from_str(&content)
                .context("Failed to parse workspace file")?;

            // Add sidecars
            for path in &self.sidecar_paths {
                workspace.folders.insert(
                    0,
                    WorkspaceFolder {
                        path: path.to_string_lossy().to_string(),
                        name: Some(format!("[Sidecar] {}", path.file_name().unwrap_or_default().to_string_lossy())),
                    },
                );
            }

            // Write temporarily
            let workspace_json = serde_json::to_string_pretty(&workspace)
                .context("Failed to serialize workspace")?;

            fs::write(&workspace_path, workspace_json)
                .context("Failed to write workspace file")?;
        }

        println!("{}", "Opening workspace in VS Code...".bright_cyan());
        println!("  {} {}", "File:".dimmed(), workspace_path.display().to_string().bright_blue());

        // Open in VS Code
        let output = Command::new("code")
            .arg(&workspace_path)
            .output()
            .context("Failed to open VS Code")?;

        if !output.status.success() {
            return Err(anyhow!("Failed to open VS Code: {}", String::from_utf8_lossy(&output.stderr)));
        }

        Ok(())
    }

    pub fn clone_from(source_path: &Path, target_paths: &[PathBuf]) -> Result<Self> {
        // Find workspace containing source_path
        let source_workspace = Self::find_workspace_containing(source_path)?;

        if source_workspace.is_none() {
            return Err(anyhow!("No workspace found containing path: {}", source_path.display()));
        }

        let source_workspace = source_workspace.unwrap();

        println!("Cloning from workspace: {}",
            source_workspace.hash.bright_yellow()
        );

        // Create new workspace (never create READMEs when cloning)
        let target_workspace = Self::new(target_paths.to_vec(), vec![], false)?;

        // Copy safe pocket contents
        if target_workspace.pocket_dir.exists() {
            fs::remove_dir_all(&target_workspace.pocket_dir)
                .context("Failed to remove existing target pocket")?;
        }

        copy_dir_all(&source_workspace.pocket_dir, &target_workspace.pocket_dir)
            .context("Failed to copy safe pocket contents")?;

        // Update workspace file with new paths
        target_workspace.create_workspace_file()?;

        println!("{} {}",
            "Cloned to:".bright_green(),
            target_workspace.hash.bright_yellow()
        );

        Ok(target_workspace)
    }

    pub fn find_workspace_containing(path: &Path) -> Result<Option<Self>> {
        let spocket_dir = Self::spocket_dir()?;

        let entries = fs::read_dir(&spocket_dir)
            .context("Failed to read .spocket directory")?;

        for entry in entries {
            let entry = entry?;
            let pocket_dir = entry.path();

            if !pocket_dir.is_dir() {
                continue;
            }

            let hash = pocket_dir.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();

            let workspace_file = pocket_dir.join(format!("{}.code-workspace", hash));

            if !workspace_file.exists() {
                continue;
            }

            let content = fs::read_to_string(&workspace_file)?;
            let workspace: VSCodeWorkspace = serde_json::from_str(&content)?;

            // Check if any folder matches the path
            for folder in &workspace.folders {
                let folder_path = PathBuf::from(&folder.path);
                if folder_path == path {
                    // Reconstruct workspace
                    let core_paths: Vec<PathBuf> = workspace
                        .folders
                        .iter()
                        .map(|f| PathBuf::from(&f.path))
                        .filter(|p| !p.starts_with(&pocket_dir))
                        .collect();

                    return Ok(Some(Self {
                        hash,
                        core_paths,
                        sidecar_paths: vec![],
                        pocket_dir,
                        create_readmes: false,
                    }));
                }
            }
        }

        Ok(None)
    }

    pub fn list_all() -> Result<Vec<Self>> {
        let spocket_dir = Self::spocket_dir()?;

        let mut workspaces = Vec::new();

        let entries = fs::read_dir(&spocket_dir)
            .context("Failed to read .spocket directory")?;

        for entry in entries {
            let entry = entry?;
            let pocket_dir = entry.path();

            if !pocket_dir.is_dir() {
                continue;
            }

            let hash = pocket_dir.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();

            let workspace_file = pocket_dir.join(format!("{}.code-workspace", hash));

            if !workspace_file.exists() {
                continue;
            }

            let content = fs::read_to_string(&workspace_file)?;
            let workspace: VSCodeWorkspace = serde_json::from_str(&content)?;

            let core_paths: Vec<PathBuf> = workspace
                .folders
                .iter()
                .map(|f| PathBuf::from(&f.path))
                .filter(|p| !p.starts_with(&pocket_dir))
                .collect();

            workspaces.push(Self {
                hash,
                core_paths,
                sidecar_paths: vec![],
                pocket_dir,
                create_readmes: false,
            });
        }

        Ok(workspaces)
    }

    /// Calculate similarity between two sets of paths
    /// Returns a score between 0.0 (no overlap) and 1.0 (identical)
    pub fn calculate_similarity(paths1: &[PathBuf], paths2: &[PathBuf]) -> f64 {
        if paths1.is_empty() || paths2.is_empty() {
            return 0.0;
        }

        let set1: HashSet<_> = paths1.iter().collect();
        let set2: HashSet<_> = paths2.iter().collect();

        let intersection = set1.intersection(&set2).count();
        let union = set1.union(&set2).count();

        intersection as f64 / union as f64
    }

    /// Find workspaces similar to the given paths
    /// Returns workspaces sorted by similarity (most similar first)
    pub fn find_similar_workspaces(target_paths: &[PathBuf], min_similarity: f64) -> Result<Vec<(Self, f64)>> {
        let all_workspaces = Self::list_all()?;
        let mut similar: Vec<(Self, f64)> = Vec::new();

        for workspace in all_workspaces {
            let similarity = Self::calculate_similarity(target_paths, &workspace.core_paths);

            if similarity >= min_similarity && similarity < 1.0 {
                similar.push((workspace, similarity));
            }
        }

        // Sort by similarity (descending)
        similar.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

        Ok(similar)
    }

    /// Prompt user to select a workspace to clone from
    /// Returns the selected workspace or None if user cancels
    pub fn prompt_clone_selection(candidates: &[(Self, f64)]) -> Result<Option<&Self>> {
        if candidates.is_empty() {
            return Ok(None);
        }

        println!("\n{}", "Similar workspaces found!".bright_white());
        println!("{}", "These workspaces share directories with your new workspace:".dimmed());
        println!();

        for (i, (workspace, similarity)) in candidates.iter().enumerate() {
            let percentage = (similarity * 100.0) as u32;
            println!("  {}. {} {}% similarity",
                (i + 1).to_string().bright_yellow(),
                workspace.hash.bright_blue(),
                percentage.to_string().bright_green()
            );

            // Show shared directories
            for path in &workspace.core_paths {
                println!("     - {}",
                    path.display().to_string().dimmed()
                );
            }
            println!();
        }

        println!("  {}. {}", "0".bright_yellow(), "Don't clone (create fresh workspace)".dimmed());
        println!();

        print!("{} ", "Select a workspace to clone from (0-{}, or press Enter to skip):".bright_white());
        use std::io::{self, Write};
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let input = input.trim();

        if input.is_empty() || input == "0" {
            return Ok(None);
        }

        if let Ok(selection) = input.parse::<usize>() {
            if selection > 0 && selection <= candidates.len() {
                return Ok(Some(&candidates[selection - 1].0));
            }
        }

        println!("{}", "Invalid selection, creating fresh workspace".bright_yellow());
        Ok(None)
    }
}

fn copy_dir_all(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)?;

    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if ty.is_dir() {
            copy_dir_all(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_similarity_identical() {
        let paths1 = vec![
            PathBuf::from("/path/a"),
            PathBuf::from("/path/b"),
        ];
        let paths2 = vec![
            PathBuf::from("/path/b"),
            PathBuf::from("/path/a"),
        ];

        let similarity = Workspace::calculate_similarity(&paths1, &paths2);
        assert_eq!(similarity, 1.0);
    }

    #[test]
    fn test_calculate_similarity_partial() {
        let paths1 = vec![
            PathBuf::from("/path/a"),
            PathBuf::from("/path/b"),
        ];
        let paths2 = vec![
            PathBuf::from("/path/a"),
            PathBuf::from("/path/c"),
        ];

        let similarity = Workspace::calculate_similarity(&paths1, &paths2);
        // 1 common out of 3 total = 1/3 ≈ 0.33
        assert!((similarity - 0.333).abs() < 0.01);
    }

    #[test]
    fn test_calculate_similarity_no_overlap() {
        let paths1 = vec![
            PathBuf::from("/path/a"),
        ];
        let paths2 = vec![
            PathBuf::from("/path/b"),
        ];

        let similarity = Workspace::calculate_similarity(&paths1, &paths2);
        assert_eq!(similarity, 0.0);
    }

    #[test]
    fn test_calculate_similarity_subset() {
        let paths1 = vec![
            PathBuf::from("/path/a"),
            PathBuf::from("/path/b"),
            PathBuf::from("/path/c"),
        ];
        let paths2 = vec![
            PathBuf::from("/path/a"),
            PathBuf::from("/path/b"),
        ];

        let similarity = Workspace::calculate_similarity(&paths1, &paths2);
        // 2 common out of 3 total = 2/3 ≈ 0.67
        assert!((similarity - 0.667).abs() < 0.01);
    }

    #[test]
    fn test_calculate_similarity_empty() {
        let paths1: Vec<PathBuf> = vec![];
        let paths2 = vec![PathBuf::from("/path/a")];

        let similarity = Workspace::calculate_similarity(&paths1, &paths2);
        assert_eq!(similarity, 0.0);
    }
}
