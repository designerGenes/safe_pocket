use clap::{Parser, Subcommand, ValueEnum};
use clap_complete::Shell;

// ── Top-level CLI ─────────────────────────────────────────────────────────────

#[derive(Parser, Debug)]
#[command(name = "spocket")]
#[command(version)]
#[command(
    about = "Safe Pocket — ad hoc VS Code workspace manager with AI copilot support",
    long_about = "\
Safe Pocket keeps \"meta\" files (copilot instructions, prompts, observations, \
feature notes) in a dedicated pocket directory (~/.safe_pocket/<hash>/) so they \
never pollute your project repo, yet VS Code opens them together with your project \
as a single multi-root workspace.

QUICK START

  # Create or open a workspace for the current directory
  spocket -i .

  # Create a workspace spanning two projects
  spocket -i ~/dev/frontend -i ~/dev/backend

  # Upgrade the pocket's template files to match your latest templates
  spocket -u ~/dev/myproject

  # Generate and install shell completions (zsh example)
  spocket completions zsh > ~/.zsh/completions/_spocket

POCKET DIRECTORY

  Pockets are stored in ~/.safe_pocket/<hash>/.

TEMPLATES

  Customise the files written into every new pocket by editing templates in
  ~/.config/safe_pocket/templates/.  Each template file must begin with:

    #SPOCKET_TEMPLATE_DESTINATION: <relative-path>

  Supported variables: {{SPOCKET_ROOT}}, {{PROJECT_ROOT}}, {{SPOCKET_NAME}}

  Directory structure is controlled by ~/.config/safe_pocket/directory_structure.md."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Add a directory to the workspace (repeatable)
    ///
    /// Resolves aliases registered with `spocket register`. Can be specified
    /// multiple times to create a multi-root workspace:
    ///
    ///   spocket -i ~/dev/api -i ~/dev/frontend
    #[arg(short = 'i', long = "include", value_name = "PATH")]
    pub include: Vec<String>,

    /// Add a temporary sidecar directory (not saved to workspace file)
    ///
    /// Sidecar directories are injected into the workspace for this session only
    /// and are removed next time the workspace is opened normally. Useful for
    /// pulling in a dependency or reference repo without permanently changing the
    /// workspace definition.
    #[arg(short = 's', long = "sidecar", value_name = "PATH")]
    pub sidecar: Vec<String>,

    /// Clone the pocket from the workspace that contains this path
    ///
    /// Copies all meta files (copilot instructions, prompts, observations, etc.)
    /// from the source pocket into the new one, then tracks lineage in both
    /// manifests. Useful when starting a new project that should inherit the
    /// AI configuration of a related one.
    ///
    ///   spocket -i ~/dev/new-project --clone-from ~/dev/existing-project
    #[arg(long = "clone-from", value_name = "PATH")]
    pub clone_from: Option<String>,

    /// Enable an optional feature
    ///
    /// Currently supported values:
    ///   beads   Initialise a Beads (bd) issue-tracking database in the pocket
    ///           and plant a .beads/redirect stub in each project directory.
    ///
    ///   spocket -i . --use beads
    #[arg(long = "use", value_name = "FEATURE")]
    pub use_features: Vec<String>,

    /// Skip creating README files in empty directories
    ///
    /// By default spocket writes helpful README.md files into new empty
    /// directories (observations/, .github/prompts/, etc.).  Pass this flag
    /// to suppress them, e.g. when cloning a pocket for a minimal setup.
    #[arg(long = "no-readme")]
    pub no_readme: bool,

    /// Upgrade an existing pocket to match current templates (does not open VS Code)
    ///
    /// Reads every template from ~/.config/safe_pocket/templates/, expands
    /// {{SPOCKET_ROOT}} / {{PROJECT_ROOT}} / {{SPOCKET_NAME}} variables, and
    /// writes the result to the pocket.  If a file already exists with different
    /// content you are shown a diff and asked to confirm before overwriting.
    ///
    /// PATH may be either:
    ///   • The pocket directory itself  (~/.safe_pocket/abc123)
    ///   • Any project directory whose pocket you want to upgrade
    ///
    ///   spocket -u ~/dev/myproject
    ///   spocket -u ~/.safe_pocket/abc123
    #[arg(short = 'u', long = "upgrade", value_name = "PATH")]
    pub upgrade: Option<String>,

    /// Force creation of a new workspace even if one already exists
    ///
    /// By default, if `spocket -i .` detects that the current directory (or any
    /// included path) already belongs to an existing safe pocket, it opens that
    /// pocket instead of creating a duplicate.  Pass `--new` to override this
    /// behaviour and always create a fresh workspace.
    ///
    ///   spocket -i . --new
    #[arg(long = "new")]
    pub force_new: bool,

    /// Enable verbose output
    ///
    /// Shows informational messages that are hidden by default, such as runtime
    /// merge notifications, template installation notices, and other
    /// non-error/non-warning details.
    ///
    ///   spocket -i . --verbose
    #[arg(long = "verbose")]
    pub verbose: bool,

    #[arg(short = 'v', hide = true)]
    pub short_version: bool,
}

// ── Subcommands ───────────────────────────────────────────────────────────────

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Register a short alias for a directory path
    ///
    /// Aliases let you refer to long directory paths by a short name in any
    /// spocket command that accepts a PATH argument.
    ///
    ///   spocket register api="~/dev/my-api-project"
    ///   spocket -i api          # same as -i ~/dev/my-api-project
    #[command(name = "register")]
    Register {
        /// Alias definition in format: name="path"
        #[arg(value_name = "NAME=PATH")]
        alias: String,
    },

    /// Remove a previously registered directory alias
    ///
    ///   spocket unregister api
    #[command(name = "unregister")]
    Unregister {
        /// Name of the alias to remove
        #[arg(value_name = "NAME")]
        name: String,
    },

    /// List all registered directory aliases
    #[command(name = "list")]
    List,

    /// List all known safe pockets with their project paths
    #[command(name = "list-workspaces")]
    ListWorkspaces,

    /// Sync the manifest after the workspace file is edited externally
    ///
    /// Called automatically by the VS Code extension whenever the active
    /// workspace changes. Updates manifest.json to reflect the current set of
    /// folders in the .code-workspace file. Outputs JSON so the extension can
    /// read the result.
    ///
    /// You rarely need to run this manually.
    #[command(name = "sync")]
    Sync {
        /// Path to the pocket directory containing the manifest and workspace file
        #[arg(long = "pocket", value_name = "PATH")]
        pocket: String,
    },

    /// Add or remove project directories from the current workspace in-place
    ///
    /// Rewrites the .code-workspace file and manifest without moving the pocket
    /// directory. Run this from inside a pocket or project directory that
    /// belongs to an existing workspace.
    ///
    ///   spocket augment --add ~/dev/new-service
    ///   spocket augment --remove ~/dev/old-service
    ///   spocket augment --add ~/dev/new-service --no-open
    #[command(name = "augment")]
    Augment {
        /// Project directory to add to the workspace
        #[arg(long = "add", value_name = "PATH")]
        add: Vec<String>,

        /// Project directory to remove from the workspace
        #[arg(long = "remove", value_name = "PATH")]
        remove: Vec<String>,

        /// Update the workspace without opening VS Code afterwards
        #[arg(long = "no-open")]
        no_open: bool,
    },

    /// Inject runtime content into destination files (called by the VS Code extension on open)
    ///
    /// For each template marked with `#SPOCKET_MERGE_AT_RUNTIME`, injects the expanded
    /// template content into the destination file wrapped in runtime markers.
    #[command(name = "merge-start")]
    MergeStart {
        /// Path to the pocket directory containing the manifest
        #[arg(long = "pocket", value_name = "PATH")]
        pocket: String,
    },

    /// Strip runtime content from destination files (called by the VS Code extension on close)
    ///
    /// Removes any content between `#SPOCKET_RUNTIME_CONTENT_START` and
    /// `#SPOCKET_RUNTIME_CONTENT_END` markers from destination files.
    #[command(name = "merge-stop")]
    MergeStop {
        /// Path to the pocket directory containing the manifest
        #[arg(long = "pocket", value_name = "PATH")]
        pocket: String,
    },

    /// Print a shell completion script to stdout
    ///
    /// Generates tab-completion definitions for your shell.  Pipe the output to
    /// the appropriate location for your shell, then source it.
    ///
    /// BASH
    ///   spocket completions bash > ~/.local/share/bash-completion/completions/spocket
    ///
    /// ZSH  (add ~/.zsh/completions to fpath first)
    ///   spocket completions zsh > ~/.zsh/completions/_spocket
    ///
    /// FISH
    ///   spocket completions fish > ~/.config/fish/completions/spocket.fish
    ///
    /// POWERSHELL
    ///   spocket completions powershell >> $PROFILE
    ///
    /// ELVISH
    ///   spocket completions elvish >> ~/.config/elvish/rc.elv
    #[command(name = "completions")]
    Completions {
        /// Shell to generate completions for
        #[arg(value_name = "SHELL")]
        shell: ShellChoice,
    },
}

// ── Shell choice enum ─────────────────────────────────────────────────────────

/// Supported shells for tab-completion generation.
#[derive(Debug, Clone, ValueEnum)]
pub enum ShellChoice {
    Bash,
    Zsh,
    Fish,
    PowerShell,
    Elvish,
}

impl From<ShellChoice> for Shell {
    fn from(s: ShellChoice) -> Self {
        match s {
            ShellChoice::Bash => Shell::Bash,
            ShellChoice::Zsh => Shell::Zsh,
            ShellChoice::Fish => Shell::Fish,
            ShellChoice::PowerShell => Shell::PowerShell,
            ShellChoice::Elvish => Shell::Elvish,
        }
    }
}
