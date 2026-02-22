mod cli;
mod config;
mod hash;
mod manifest;
mod workspace;

use anyhow::{anyhow, Context, Result};
use clap::Parser;
use colored::Colorize;
use std::fs;
use std::path::PathBuf;

use cli::{Cli, Commands};
use config::Config;
use manifest::Manifest;
use workspace::{DriftResult, Workspace};

fn main() {
    if let Err(e) = run() {
        eprintln!("{} {}", "Error:".bright_red(), e);
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    // Handle subcommands first
    if let Some(command) = cli.command {
        return handle_command(command);
    }

    // If no subcommand, we're creating/opening a workspace
    if cli.include.is_empty() {
        return Err(anyhow!("No directories specified. Use -i/--include to add directories."));
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

            println!("{} {} -> {}",
                "Registered alias:".bright_green(),
                name.bright_yellow(),
                path.dimmed()
            );

            Ok(())
        }

        Commands::Unregister { name } => {
            let mut config = Config::load()?;

            if config.unregister_alias(&name)? {
                println!("{} {}",
                    "Unregistered alias:".bright_green(),
                    name.bright_yellow()
                );
            } else {
                println!("{} {}",
                    "Alias not found:".dimmed(),
                    name.bright_yellow()
                );
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
                println!("  {} -> {}",
                    name.bright_yellow(),
                    path.bright_blue()
                );
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
                println!("  {} {}",
                    workspace.hash.bright_yellow(),
                    format!("({})", workspace.pocket_dir.display()).dimmed()
                );

                for path in &workspace.core_paths {
                    println!("    - {}",
                        path.display().to_string().bright_blue()
                    );
                }

                println!();
            }

            Ok(())
        }

        Commands::Augment { add, remove, no_open } => {
            handle_augment(add, remove, no_open)
        }
    }
}

fn handle_workspace(cli: Cli) -> Result<()> {
    let config = Config::load()?;

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
            return Err(anyhow!("Sidecar path does not exist: {}", resolved.display()));
        }

        sidecar_paths.push(resolved);
    }

    // Handle clone-from
    if let Some(clone_from) = cli.clone_from {
        let source_path = config.resolve_path(&clone_from)?;

        let workspace = Workspace::clone_from(&source_path, &core_paths)?;
        workspace.open()?;

        return Ok(());
    }

    // Create or open workspace
    let create_readmes = !cli.no_readme;
    let workspace = Workspace::new(core_paths.clone(), sidecar_paths, create_readmes)?;

    if !workspace.exists() {
        // Check for similar workspaces (smart cloning)
        let similar_workspaces = Workspace::find_similar_workspaces(&core_paths, 0.3)?;

        if !similar_workspaces.is_empty() {
            if let Some(selected) = Workspace::prompt_clone_selection(&similar_workspaces)? {
                // Clone from selected workspace
                println!("Cloning from: {}",
                    selected.hash.bright_yellow()
                );

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

                println!("{} {}",
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

        workspace.open()?;
    } else {
        println!("{}", "Using existing workspace".dimmed());

        // Drift detection
        let drift_result = workspace.detect_and_resolve_drift()?;

        match drift_result {
            DriftResult::AcceptFile { new_core_paths, existing_workspace } => {
                let migrated = Workspace::migrate_pocket(
                    &workspace,
                    new_core_paths,
                    Some(&existing_workspace),
                )?;
                migrated.open()?;
            }
            _ => {
                workspace.open()?;
            }
        }
    }

    Ok(())
}

fn handle_augment(add: Vec<String>, remove: Vec<String>, no_open: bool) -> Result<()> {
    if add.is_empty() && remove.is_empty() {
        return Err(anyhow!("Nothing to do. Use --add or --remove to modify the workspace."));
    }

    let config = Config::load()?;
    let cwd = std::env::current_dir()
        .context("Failed to get current working directory")?;

    let workspace = Workspace::find_workspace_for_cwd(&cwd)?
        .ok_or_else(|| anyhow!(
            "No workspace found for current directory: {}\nRun this from inside a workspace directory or a pocket directory.",
            cwd.display()
        ))?;

    println!("{} {}",
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
            println!("  {} {} (already in workspace)",
                "Skipped:".dimmed(),
                resolved.display().to_string().bright_blue()
            );
        } else {
            println!("  {} {}",
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
            println!("  {} {}",
                "Removing:".bright_red(),
                resolved.display().to_string().bright_blue()
            );
        } else {
            println!("  {} {} (not in workspace)",
                "Skipped:".dimmed(),
                resolved.display().to_string().bright_blue()
            );
        }
    }

    // Validate
    if new_paths.is_empty() {
        return Err(anyhow!("Cannot remove all directories. At least one directory must remain."));
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
        let (ws, _) = Workspace::read_workspace_file(&workspace_file, &workspace.pocket_dir)?;
        Some(ws)
    } else {
        None
    };

    // Migrate
    let migrated = Workspace::migrate_pocket(
        &workspace,
        new_paths,
        existing_ws.as_ref(),
    )?;

    if !no_open {
        migrated.open()?;
    } else {
        println!("{} {}", "New workspace:".bright_green(), migrated.hash.bright_yellow());
        println!("  {} {}", "Location:".dimmed(), migrated.pocket_dir.display().to_string().bright_blue());
    }

    Ok(())
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
