mod cli;
mod config;
mod hash;
mod manifest;
mod template;
mod workspace;

use anyhow::{anyhow, Context, Result};
use clap::{CommandFactory, Parser};
use clap_complete::generate;
use colored::Colorize;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::sync::OnceLock;

use cli::{Cli, Commands};
use config::Config;
use manifest::Manifest;
use workspace::{DriftResult, Workspace};

static VERBOSE: OnceLock<bool> = OnceLock::new();

pub fn verbose() -> bool {
    *VERBOSE.get().unwrap_or(&false)
}

fn main() {
    if let Err(e) = run() {
        eprintln!("{} {}", "Error:".bright_red(), e);
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    let _ = VERBOSE.set(cli.verbose);

    if cli.short_version {
        println!("{}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    // Ensure default template assets exist in the user's config directory.
    // This is a no-op after the first run (only writes files that don't exist).
    // Silently ignore errors here so a broken config dir doesn't block normal use.
    let _ = template::ensure_default_assets();

    // Handle subcommands first
    if let Some(command) = cli.command {
        return handle_command(command);
    }

    // Handle upgrade (-u)
    if let Some(upgrade_path) = cli.upgrade {
        return handle_upgrade(upgrade_path);
    }

    // If no subcommand, we're creating/opening a workspace
    if cli.include.is_empty() {
        return Err(anyhow!(
            "No directories specified. Use -i/--include to add directories."
        ));
    }

    handle_workspace(cli)
}

fn handle_command(command: Commands) -> Result<()> {
    match command {
        Commands::Register { alias } => {
            let parts: Vec<&str> = alias.splitn(2, '=').collect();

            if parts.len() != 2 {
                return Err(anyhow!("Invalid alias format. Use: name=\"path\""));
            }

            let name = parts[0].trim().to_string();
            let path = parts[1].trim().trim_matches('"').to_string();

            let mut config = Config::load()?;
            config.register_alias(name.clone(), path.clone())?;

            println!(
                "{} {} -> {}",
                "Registered alias:".bright_green(),
                name.bright_yellow(),
                path.dimmed()
            );

            Ok(())
        }

        Commands::Unregister { name } => {
            let mut config = Config::load()?;

            if config.unregister_alias(&name)? {
                println!(
                    "{} {}",
                    "Unregistered alias:".bright_green(),
                    name.bright_yellow()
                );
            } else {
                println!("{} {}", "Alias not found:".dimmed(), name.bright_yellow());
            }

            Ok(())
        }

        Commands::List => {
            let config = Config::load()?;

            if config.aliases.is_empty() {
                println!("{}", "No aliases registered.".dimmed());
                return Ok(());
            }

            println!("{}", "Registered aliases:".bright_white().bold());
            println!();

            let mut aliases: Vec<_> = config.aliases.iter().collect();
            aliases.sort_by_key(|(name, _)| *name);

            for (name, path) in aliases {
                println!("  {} -> {}", name.bright_yellow(), path.bright_blue());
            }

            Ok(())
        }

        Commands::ListWorkspaces => {
            let workspaces = Workspace::list_all()?;

            if workspaces.is_empty() {
                println!("{}", "No workspaces found.".dimmed());
                return Ok(());
            }

            println!("{}", "Workspaces:".bright_white().bold());
            println!();

            for workspace in workspaces {
                println!(
                    "  {} {}",
                    workspace.hash.bright_yellow(),
                    format!("({})", workspace.pocket_dir.display()).dimmed()
                );

                for path in &workspace.core_paths {
                    println!("    - {}", path.display().to_string().bright_blue());
                }

                println!();
            }

            Ok(())
        }

        Commands::Sync { pocket } => handle_sync(pocket),

        Commands::MergeStart { pocket } => handle_merge_start(pocket),

        Commands::MergeStop { pocket } => handle_merge_stop(pocket),

        Commands::Augment {
            add,
            remove,
            no_open,
        } => handle_augment(add, remove, no_open),

        Commands::Completions { shell } => {
            let mut cmd = Cli::command();
            let shell: clap_complete::Shell = shell.into();
            generate(shell, &mut cmd, "spocket", &mut io::stdout());
            Ok(())
        }
    }
}

fn handle_workspace(cli: Cli) -> Result<()> {
    let config = Config::load()?;

    // Parse --use features (normalised to lowercase)
    let use_beads = cli.use_features.iter().any(|f| f.to_lowercase() == "beads");

    // Validate unknown --use values
    for feature in &cli.use_features {
        if feature.to_lowercase() != "beads" {
            return Err(anyhow!(
                "Unknown feature '{}'. Supported values: beads",
                feature
            ));
        }
    }

    // Resolve core paths
    let mut core_paths = Vec::new();
    for path_str in &cli.include {
        let resolved = config.resolve_path(path_str)?;

        if !resolved.exists() {
            return Err(anyhow!("Path does not exist: {}", resolved.display()));
        }

        core_paths.push(resolved);
    }

    // Resolve sidecar paths
    let mut sidecar_paths = Vec::new();
    for path_str in &cli.sidecar {
        let resolved = config.resolve_path(path_str)?;

        if !resolved.exists() {
            return Err(anyhow!(
                "Sidecar path does not exist: {}",
                resolved.display()
            ));
        }

        sidecar_paths.push(resolved);
    }

    // Handle clone-from
    if let Some(clone_from) = cli.clone_from {
        let source_path = config.resolve_path(&clone_from)?;

        let workspace = Workspace::clone_from(&source_path, &core_paths)?;

        if use_beads {
            workspace.setup_beads()?;
        }

        open_with_merge(&workspace)?;

        return Ok(());
    }

    // Create or open workspace
    let create_readmes = !cli.no_readme;
    let workspace = Workspace::new(core_paths.clone(), sidecar_paths, create_readmes)?;

    if !workspace.exists() {
        // Secondary lookup: check if any existing pocket's manifest matches these paths
        // (handles pockets that evolved in-place via sync/augment)
        if let Some(existing) = Workspace::find_workspace_by_manifest_paths(&core_paths)? {
            println!(
                "{} {} (matched by manifest)",
                "Found existing pocket:".bright_green(),
                existing.hash.bright_yellow()
            );

            if use_beads {
                existing.setup_beads()?;
            }

            open_with_merge(&existing)?;
            return Ok(());
        }

        // Check for similar workspaces (smart cloning)
        let similar_workspaces = Workspace::find_similar_workspaces(&core_paths, 0.3)?;

        if !similar_workspaces.is_empty() {
            if let Some(selected) = Workspace::prompt_clone_selection(&similar_workspaces)? {
                // Clone from selected workspace
                println!("Cloning from: {}", selected.hash.bright_yellow());

                // Copy safe pocket contents
                if workspace.pocket_dir.exists() {
                    fs::remove_dir_all(&workspace.pocket_dir)
                        .context("Failed to remove existing target pocket")?;
                }

                copy_dir_all(&selected.pocket_dir, &workspace.pocket_dir)
                    .context("Failed to copy safe pocket contents")?;

                // Create workspace file with new paths
                workspace.create_workspace_file()?;

                // Write manifest with lineage
                let manifest = Manifest::new_cloned(
                    workspace.hash.clone(),
                    workspace.core_paths.clone(),
                    selected.hash.clone(),
                );
                manifest.save(&workspace.pocket_dir)?;

                // Update parent's children list
                if let Ok(Some(mut parent_manifest)) = Manifest::load(&selected.pocket_dir) {
                    parent_manifest.add_child(workspace.hash.clone());
                    let _ = parent_manifest.save(&selected.pocket_dir);
                }

                println!(
                    "{} {}",
                    "Cloned to:".bright_green(),
                    workspace.hash.bright_yellow()
                );
            } else {
                // User chose not to clone
                workspace.create()?;
            }
        } else {
            // No similar workspaces found
            workspace.create()?;
        }

        if use_beads {
            workspace.setup_beads()?;
        }

        open_with_merge(&workspace)?;
    } else {
        println!("{}", "Using existing workspace".dimmed());

        workspace.migrate_storage_references()?;

        // Run beads setup regardless of whether the pocket is new — idempotent
        if use_beads {
            workspace.setup_beads()?;
        }

        // Drift detection
        let drift_result = workspace.detect_and_resolve_drift()?;

        match drift_result {
            DriftResult::AcceptFile { new_core_paths } => {
                let mut manifest = match Manifest::load(&workspace.pocket_dir)? {
                    Some(m) => m,
                    None => Manifest::new(workspace.hash.clone(), workspace.core_paths.clone()),
                };
                manifest.update_paths(new_core_paths, &workspace.pocket_dir)?;
                println!(
                    "{} {}",
                    "Manifest updated in place:".bright_green(),
                    manifest.hash.bright_yellow()
                );
                open_with_merge(&workspace)?;
            }
            _ => {
                open_with_merge(&workspace)?;
            }
        }
    }

    Ok(())
}

fn handle_sync(pocket: String) -> Result<()> {
    let pocket_dir = PathBuf::from(&pocket);

    if !pocket_dir.is_dir() {
        let out = serde_json::json!({
            "status": "error",
            "message": format!("Pocket directory does not exist: {}", pocket)
        });
        println!("{}", serde_json::to_string(&out)?);
        return Ok(());
    }

    // Find the workspace file
    let workspace_file = match Workspace::find_workspace_file(&pocket_dir) {
        Some(f) => f,
        None => {
            let out = serde_json::json!({
                "status": "error",
                "message": "No workspace file found in pocket directory"
            });
            println!("{}", serde_json::to_string(&out)?);
            return Ok(());
        }
    };

    let workspace_hash = pocket_dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("")
        .to_string();
    let migration_workspace = Workspace {
        hash: workspace_hash,
        core_paths: vec![],
        sidecar_paths: vec![],
        pocket_dir: pocket_dir.clone(),
        create_readmes: false,
    };
    migration_workspace.migrate_storage_references()?;

    // Read current paths from workspace file
    let (_, file_paths) = Workspace::read_workspace_file(&workspace_file, &pocket_dir)?;

    // Load or backfill manifest
    let (mut manifest, manifest_paths) = match Workspace::load_manifest_or_backfill(&pocket_dir)? {
        Some(result) => result,
        None => {
            let out = serde_json::json!({
                "status": "error",
                "message": "Failed to load or create manifest"
            });
            println!("{}", serde_json::to_string(&out)?);
            return Ok(());
        }
    };

    // Compare
    let file_set: std::collections::HashSet<_> = file_paths.iter().collect();
    let manifest_set: std::collections::HashSet<_> = manifest_paths.iter().collect();

    if file_set == manifest_set {
        let out = serde_json::json!({
            "status": "unchanged",
            "hash": manifest.hash,
            "birth_hash": manifest.birth_hash(),
            "paths": manifest.core_paths,
        });
        println!("{}", serde_json::to_string(&out)?);
        return Ok(());
    }

    // Paths differ — update manifest in place
    let old_hash = manifest.hash.clone();
    manifest.update_paths(file_paths, &pocket_dir)?;

    let out = serde_json::json!({
        "status": "synced",
        "old_hash": old_hash,
        "new_hash": manifest.hash,
        "birth_hash": manifest.birth_hash(),
        "paths": manifest.core_paths,
    });
    println!("{}", serde_json::to_string(&out)?);
    Ok(())
}

fn handle_augment(add: Vec<String>, remove: Vec<String>, no_open: bool) -> Result<()> {
    if add.is_empty() && remove.is_empty() {
        return Err(anyhow!(
            "Nothing to do. Use --add or --remove to modify the workspace."
        ));
    }

    let config = Config::load()?;
    let cwd = std::env::current_dir().context("Failed to get current working directory")?;

    let workspace = Workspace::find_workspace_for_cwd(&cwd)?
        .ok_or_else(|| anyhow!(
            "No workspace found for current directory: {}\nRun this from inside a workspace directory or a pocket directory.",
            cwd.display()
        ))?;

    println!(
        "{} {}",
        "Found workspace:".bright_white(),
        workspace.hash.bright_yellow()
    );

    // Build new core_paths
    let mut new_paths: Vec<PathBuf> = workspace.core_paths.clone();

    // Process additions
    for path_str in &add {
        let resolved = config.resolve_path(path_str)?;

        if !resolved.exists() {
            return Err(anyhow!("Path does not exist: {}", resolved.display()));
        }

        if new_paths.contains(&resolved) {
            println!(
                "  {} {} (already in workspace)",
                "Skipped:".dimmed(),
                resolved.display().to_string().bright_blue()
            );
        } else {
            println!(
                "  {} {}",
                "Adding:".bright_green(),
                resolved.display().to_string().bright_blue()
            );
            new_paths.push(resolved);
        }
    }

    // Process removals
    for path_str in &remove {
        let resolved = config.resolve_path(path_str)?;
        let before_len = new_paths.len();
        new_paths.retain(|p| p != &resolved);

        if new_paths.len() < before_len {
            println!(
                "  {} {}",
                "Removing:".bright_red(),
                resolved.display().to_string().bright_blue()
            );
        } else {
            println!(
                "  {} {} (not in workspace)",
                "Skipped:".dimmed(),
                resolved.display().to_string().bright_blue()
            );
        }
    }

    // Validate
    if new_paths.is_empty() {
        return Err(anyhow!(
            "Cannot remove all directories. At least one directory must remain."
        ));
    }

    // Check if anything actually changed
    let new_paths_set: std::collections::HashSet<_> = new_paths.iter().collect();
    let old_paths_set: std::collections::HashSet<_> = workspace.core_paths.iter().collect();

    if new_paths_set == old_paths_set {
        println!("{}", "No changes to apply.".dimmed());
        return Ok(());
    }

    // Read existing workspace file for settings preservation
    let workspace_file = workspace.workspace_file_path();
    let existing_ws = if workspace_file.exists() {
        workspace.migrate_storage_references()?;
        let (ws, _) = Workspace::read_workspace_file(&workspace_file, &workspace.pocket_dir)?;
        Some(ws)
    } else {
        None
    };

    // In-place update: rewrite workspace file + manifest, pocket dir stays put
    let updated_workspace = Workspace {
        hash: workspace.hash.clone(),
        core_paths: new_paths.clone(),
        sidecar_paths: workspace.sidecar_paths.clone(),
        pocket_dir: workspace.pocket_dir.clone(),
        create_readmes: false,
    };
    updated_workspace.write_workspace_file_preserving(existing_ws.as_ref())?;

    // Update manifest in place
    let mut manifest = match Manifest::load(&workspace.pocket_dir)? {
        Some(m) => m,
        None => Manifest::new(workspace.hash.clone(), workspace.core_paths.clone()),
    };
    manifest.update_paths(new_paths, &workspace.pocket_dir)?;

    println!(
        "{} {} (pocket dir unchanged)",
        "Workspace updated in place:".bright_green(),
        manifest.hash.bright_yellow()
    );

    if !no_open {
        open_with_merge(&updated_workspace)?;
    } else {
        println!(
            "  {} {}",
            "Location:".dimmed(),
            workspace.pocket_dir.display().to_string().bright_blue()
        );
    }

    Ok(())
}

fn open_with_merge(ws: &Workspace) -> Result<()> {
    if let Ok(ctx) = build_template_context(&ws.pocket_dir) {
        if let Err(e) = template::apply_merge_at_runtime(&ws.pocket_dir, &ctx) {
            eprintln!(
                "{} {}",
                "Warning: merge-at-runtime failed:".bright_yellow(),
                e
            );
        }
    }
    ws.open()
}

fn build_template_context(pocket_dir: &std::path::Path) -> Result<template::TemplateContext> {
    let manifest = Manifest::load(pocket_dir)?
        .ok_or_else(|| anyhow!("No manifest found in pocket: {}", pocket_dir.display()))?;

    let project_root = manifest
        .core_paths
        .first()
        .cloned()
        .unwrap_or_else(|| PathBuf::from("<unknown>"));

    let spocket_name = pocket_dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();

    let global_obs = template::global_observations_dir().unwrap_or_else(|_| {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("/"))
            .join(".config")
            .join("safe_pocket")
            .join("observations")
    });

    Ok(template::TemplateContext {
        spocket_root: pocket_dir.to_path_buf(),
        project_root,
        spocket_name,
        global_observations_path: global_obs,
    })
}

fn handle_merge_start(pocket: String) -> Result<()> {
    let pocket_dir = PathBuf::from(&pocket);

    if !pocket_dir.is_dir() {
        return Err(anyhow!("Pocket directory does not exist: {}", pocket));
    }

    let ctx = build_template_context(&pocket_dir)?;
    let count = template::apply_merge_at_runtime(&pocket_dir, &ctx)?;

    if count == 0 {
        println!("{}", "No runtime merge templates found.".dimmed());
    } else {
        println!(
            "{} {} file(s) runtime-merged.",
            "Merge-start complete:".bright_green(),
            count.to_string().bright_yellow()
        );
    }

    Ok(())
}

fn handle_merge_stop(pocket: String) -> Result<()> {
    let pocket_dir = PathBuf::from(&pocket);

    if !pocket_dir.is_dir() {
        return Err(anyhow!("Pocket directory does not exist: {}", pocket));
    }

    let ctx = build_template_context(&pocket_dir)?;
    let count = template::strip_merge_at_runtime(&pocket_dir, &ctx)?;

    if count == 0 {
        println!("{}", "No runtime content to strip.".dimmed());
    } else {
        println!(
            "{} {} file(s) cleaned.",
            "Merge-stop complete:".bright_green(),
            count.to_string().bright_yellow()
        );
    }

    Ok(())
}

fn handle_upgrade(path: String) -> Result<()> {
    let config = Config::load()?;
    let resolved = config.resolve_path(&path)?;

    // The path might be:
    // 1. A pocket directory directly (e.g. ~/.safe_pocket/abc123)
    // 2. A project directory that has an associated pocket
    let spocket_dir = Workspace::spocket_dir()?;

    let pocket_dir = if resolved.starts_with(&spocket_dir) && resolved.is_dir() {
        // Direct pocket path
        resolved
    } else {
        // Try to find the pocket for this project path
        let workspace = Workspace::find_workspace_containing(&resolved)?
            .or_else(|| {
                // Also try find_workspace_for_cwd
                Workspace::find_workspace_for_cwd(&resolved).ok().flatten()
            })
            .ok_or_else(|| {
                anyhow!(
                    "No safe pocket found for path: {}\n\
                     Provide either a pocket directory or a project directory with an existing pocket.",
                    resolved.display()
                )
            })?;
        workspace.pocket_dir
    };

    template::upgrade_pocket(&pocket_dir)
}

// Helper function to copy directory contents
fn copy_dir_all(src: &std::path::Path, dst: &std::path::Path) -> Result<()> {
    use std::fs;

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
