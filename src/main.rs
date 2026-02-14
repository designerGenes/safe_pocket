mod cli;
mod config;
mod hash;
mod workspace;

use anyhow::{anyhow, Context, Result};
use clap::Parser;
use colored::Colorize;
use std::fs;

use cli::{Cli, Commands};
use config::Config;
use workspace::Workspace;

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

                // Use the internal copy function (need to make it public or call differently)
                copy_dir_all(&selected.pocket_dir, &workspace.pocket_dir)
                    .context("Failed to copy safe pocket contents")?;

                // Create workspace file with new paths
                workspace.create_workspace_file()?;

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
    } else {
        println!("{}", "Using existing workspace".dimmed());
        workspace.check_mismatch()?;
    }

    workspace.open()?;

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
