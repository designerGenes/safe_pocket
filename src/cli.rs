use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "spocket")]
#[command(about = "Safe Pocket - Ad hoc VS Code workspace manager", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Include directories in the workspace
    #[arg(short = 'i', long = "include", value_name = "PATH")]
    pub include: Vec<String>,

    /// Sidecar directories (temporary, not saved to workspace file)
    #[arg(short = 's', long = "sidecar", value_name = "PATH")]
    pub sidecar: Vec<String>,

    /// Clone safe pocket from workspace containing this path
    #[arg(long = "clone-from", value_name = "PATH")]
    pub clone_from: Option<String>,

    /// Include Beads setup (placeholder)
    #[arg(long = "with-beads")]
    pub with_beads: bool,

    /// Skip creating README files in empty directories
    #[arg(long = "no-readme")]
    pub no_readme: bool,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Register a directory alias
    #[command(name = "register")]
    Register {
        /// Alias definition in format: name="path"
        #[arg(value_name = "ALIAS")]
        alias: String,
    },

    /// Unregister a directory alias
    #[command(name = "unregister")]
    Unregister {
        /// Alias name to remove
        #[arg(value_name = "NAME")]
        name: String,
    },

    /// List all registered aliases
    #[command(name = "list")]
    List,

    /// List all workspaces
    #[command(name = "list-workspaces")]
    ListWorkspaces,
}
