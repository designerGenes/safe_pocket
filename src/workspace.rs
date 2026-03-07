use anyhow::{anyhow, Context, Result};
use colored::Colorize;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::hash::hash_paths;
use crate::manifest::Manifest;

#[derive(Debug, Serialize, Deserialize)]
pub struct VSCodeWorkspace {
    pub folders: Vec<WorkspaceFolder>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settings: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extensions: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub launch: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tasks: Option<serde_json::Value>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WorkspaceFolder {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

pub enum DriftResult {
    InSync,
    AcceptFile { new_core_paths: Vec<PathBuf> },
    OverwrittenFile,
    Skipped,
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
        let home = dirs::home_dir().context("Failed to get home directory")?;

        let spocket_dir = home.join(".spocket");

        fs::create_dir_all(&spocket_dir).context("Failed to create .spocket directory")?;

        Ok(spocket_dir)
    }

    /// Find the .code-workspace file in a pocket directory.
    /// Looks for `<dirname>.code-workspace` first, falls back to any `.code-workspace` file.
    pub fn find_workspace_file(pocket_dir: &Path) -> Option<PathBuf> {
        let dir_name = pocket_dir.file_name()?.to_str()?;
        let primary = pocket_dir.join(format!("{}.code-workspace", dir_name));
        if primary.exists() {
            return Some(primary);
        }

        // Fallback: find any .code-workspace file
        if let Ok(entries) = fs::read_dir(pocket_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("code-workspace") {
                    return Some(path);
                }
            }
        }

        None
    }

    /// Load manifest from a pocket directory, backfilling from workspace file if needed.
    /// Returns (manifest, core_paths) where core_paths come from the manifest (or workspace file on backfill).
    pub fn load_manifest_or_backfill(
        pocket_dir: &Path,
    ) -> Result<Option<(Manifest, Vec<PathBuf>)>> {
        let dir_name = pocket_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();

        if let Some(manifest) = Manifest::load(pocket_dir)? {
            let core_paths = manifest.core_paths.clone();
            return Ok(Some((manifest, core_paths)));
        }

        // No manifest — try to backfill from workspace file
        if Self::find_workspace_file(pocket_dir).is_none() {
            return Ok(None);
        }

        // Use dir name as hash for backfill
        let manifest = Manifest::backfill(pocket_dir, &dir_name)?;
        let core_paths = manifest.core_paths.clone();
        Ok(Some((manifest, core_paths)))
    }

    pub fn new(
        core_paths: Vec<PathBuf>,
        sidecar_paths: Vec<PathBuf>,
        create_readmes: bool,
    ) -> Result<Self> {
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
        self.pocket_dir
            .join(format!("{}.code-workspace", self.hash))
    }

    pub fn exists(&self) -> bool {
        self.pocket_dir.exists() && self.workspace_file_path().exists()
    }

    pub fn create(&self) -> Result<()> {
        if self.exists() {
            println!("{}", "Workspace already exists".dimmed());
            // Still do drift detection for existing workspaces
            return Ok(());
        }

        println!("{}", "Creating new safe pocket...".bright_white());

        // Create pocket directory structure
        self.create_pocket_structure()?;

        // Create workspace file
        self.create_workspace_file()?;

        // Initialize git
        self.init_git()?;

        // Write manifest
        let manifest = Manifest::new(self.hash.clone(), self.core_paths.clone());
        manifest.save(&self.pocket_dir)?;

        println!(
            "{} {}",
            "Created workspace:".bright_green(),
            self.hash.bright_yellow()
        );
        println!(
            "  {} {}",
            "Location:".dimmed(),
            self.pocket_dir.display().to_string().bright_blue()
        );

        Ok(())
    }

    fn create_pocket_structure(&self) -> Result<()> {
        fs::create_dir_all(&self.pocket_dir).context("Failed to create pocket directory")?;

        let pocket_dir_str = self.pocket_dir.to_string_lossy();

        // Build a formatted list of core paths for use in templates
        let core_paths_list = self
            .core_paths
            .iter()
            .map(|p| format!("- {}", p.display()))
            .collect::<Vec<_>>()
            .join("\n");

        // Primary project path (first core path) used in templates
        let primary_project_path = self
            .core_paths
            .first()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| "<project path>".to_string());

        // ── Root README.md ────────────────────────────────────────────────────
        if self.create_readmes {
            let readme = self.pocket_dir.join("README.md");
            fs::write(
                &readme,
                format!(
                    "# Safe Pocket: {hash}\n\n\
                    This is a Safe Pocket workspace directory. It contains:\n\n\
                    - `.github/copilot-instructions.md` - Custom AI copilot instructions\n\
                    - `.github/prompts/` - Reusable prompt templates\n\
                    - `FEATURES/` - Feature ideas and documentation\n\
                    - `observations/` - AI-generated insights and learnings\n\n\
                    ## Usage\n\n\
                    This directory is automatically managed by spocket. Edit the files above to customize \
                    your AI assistant's behavior for the workspace directories:\n\n\
                    {paths}\n\n\
                    Learn more: https://github.com/your-repo/safe_pocket\n",
                    hash = self.hash,
                    paths = core_paths_list,
                ),
            )
            .context("Failed to create README.md")?;
        }

        // ── .github/ subdirectories ───────────────────────────────────────────
        let github_dir = self.pocket_dir.join(".github");
        let github_prompts = github_dir.join("prompts");
        let github_agents = github_dir.join("agents");
        let github_skills = github_dir.join("skills");

        fs::create_dir_all(&github_prompts)
            .context("Failed to create .github/prompts directory")?;
        fs::create_dir_all(&github_agents).context("Failed to create .github/agents directory")?;
        fs::create_dir_all(&github_skills).context("Failed to create .github/skills directory")?;

        // .github/prompts/README.md
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
                Then reference it in your copilot conversations.\n",
            )
            .context("Failed to create prompts README.md")?;
        }

        // .github/copilot-instructions.md — rich templated content
        let copilot_instructions = github_dir.join("copilot-instructions.md");
        fs::write(
            &copilot_instructions,
            format!(
                "# Project folder versus safe pocket folder\n\n\
                This file is contained inside a subdirectory of\n\
                {pocket_dir}\n\n\
                That folder is a \"safe pocket\" folder. It is NOT the \"project folder\". \
                The safe pocket folder contains \"meta files\" which are relevant only to the \
                actual project folder. The actual project folder is\n\
                {project_path}\n\n\
                All commands made to the agent are intended to be applied to the project folder, \
                not the safe pocket folder. The safe pocket folder is only for storing meta files \
                that are relevant to the project folder. The agent should never make any changes \
                to the safe pocket folder, only read from it.\n\n\
                For example, if the agent is asked to review our codebase, it should read the code \
                files from the project folder, not the safe pocket folder. The safe pocket folder \
                may contain instructions or other meta files that are relevant to the project folder, \
                but the actual code files are in the project folder.\n\n\
                # Rules\n\n\
                1. You must always use full paths whenever you reference any file or directory. \
                NEVER use relative paths.\n\
                2. If instructed to use a \"cli app\" or \"terminal command\", you should run this \
                command in the context of the project folder, not the safe pocket folder. You should \
                also always try to run the literal command you are told to use, before searching for \
                python files or source code. For example, if I tell you \"use the cli app sponge_bob \
                to do X\", you must first attempt to run the command \"sponge_bob\" in the terminal, \
                and only if that fails should you search for a python file or source code that might \
                be relevant.\n\
                3. All python dependencies and environments are managed by 'uv', never by 'pip'.\n\n\
                # Observations Logging\n\n\
                As you work, you will inevitably discover significant insights about the project, \
                codebase, patterns, bugs, conventions, and other noteworthy findings. You are \
                required to actively log these as \"observation\" files in the safe pocket folder.\n\n\
                ## What qualifies as an Observation\n\n\
                Log an observation whenever you discover any of the following:\n\
                - Architectural patterns or design decisions in the codebase\n\
                - Recurring bugs, anti-patterns, or footguns\n\
                - Non-obvious conventions or project-specific idioms\n\
                - Important constraints (e.g., dependency quirks, environment limitations)\n\
                - Useful techniques or shortcuts specific to this project\n\
                - Surprising or counter-intuitive behavior you encounter\n\
                - Decisions made during a session that future sessions should know about\n\n\
                When in doubt, log it. Observations are cheap to create and valuable to retain.\n\n\
                ## Where to write Observations\n\n\
                Always write observation files to:\n\
                ```\n\
                {pocket_dir}/observations/\n\
                ```\n\n\
                This is the safe pocket folder — writing here is explicitly permitted for observation logging.\n\n\
                ## Naming Convention\n\n\
                Name each file using the following format:\n\
                ```\n\
                YYYY-MM-DD--<slug>.md\n\
                ```\n\n\
                Where `<slug>` is a short, lowercase, hyphen-separated summary of the observation's \
                subject derived from its content. The slug should be specific enough to be meaningful \
                at a glance.\n\n\
                Examples:\n\
                - `2025-06-10--uv-env-not-activated-by-default.md`\n\
                - `2025-06-10--project-uses-ruff-not-black.md`\n\
                - `2025-06-11--api-auth-token-stored-in-dotenv.md`\n\n\
                Do NOT use generic slugs like `observation-1` or `misc-finding`.\n\n\
                ## File Format\n\n\
                Each observation file should be a short Markdown file with the following structure:\n\n\
                ```markdown\n\
                # <Title of Observation>\n\n\
                **Date:** YYYY-MM-DD  \n\
                **Context:** <Brief description of what you were doing when you made this observation>\n\n\
                ## Finding\n\n\
                <Clear, concise description of what you observed.>\n\n\
                ## Why It Matters\n\n\
                <Why this is worth knowing for future sessions or contributors.>\n\n\
                ## Notes\n\n\
                <Any additional details, caveats, or related links. Omit if not needed.>\n\
                ```\n\n\
                Keep observations focused. One observation per file. Split large findings into \
                multiple files if needed.\n",
                pocket_dir = pocket_dir_str,
                project_path = primary_project_path,
            ),
        )
        .context("Failed to create copilot-instructions.md")?;

        // ── AGENTS.md (root) ──────────────────────────────────────────────────
        let agents_md = self.pocket_dir.join("AGENTS.md");
        // Build the project layout table rows
        let layout_rows = self
            .core_paths
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let label = if i == 0 {
                    "**Project folder** — all source code lives here"
                } else {
                    "**Additional project folder**"
                };
                format!("| `{}` | {} |", p.display(), label)
            })
            .collect::<Vec<_>>()
            .join("\n");

        fs::write(
            &agents_md,
            format!(
                "# Agent Instructions\n\n\
                ## Project Layout\n\n\
                | Path | Purpose |\n\
                |------|---------|\n\
                {layout_rows}\n\
                | `{pocket_dir}` | **Safe pocket** — meta/config files only (read-only) |\n\n\
                All agent work targets the **project folder**. Never modify the safe pocket folder.\n\n\
                > **Beads database location:** The `.beads/` database lives in the **safe pocket**, not the project folder.\n\
                > A stub `.beads/redirect` file in the project folder points `bd` to the correct location automatically.\n\
                > Run all `bd` commands from the project folder — they will resolve correctly via the redirect.\n\n\
                ## Absolute Rules\n\n\
                1. **Always use full absolute paths.** Never use relative paths in any file reference or command.\n\
                2. **Run CLI commands in the project folder context.** If told to use a CLI tool (e.g. `some_tool`), \
                run it literally first; only search source code if the command fails.\n\
                3. **Python dependencies use `uv`, never `pip`.** Install packages with `uv add <pkg>`, \
                run scripts with `uv run <script>`.\n\n\
                ---\n\n\
                ## Build / Lint / Test Commands\n\n\
                > No source code exists yet. Add commands here as the project grows.\n\n\
                **Python (when applicable):**\n\
                ```bash\n\
                uv run pytest                        # Run all tests\n\
                uv run pytest tests/test_foo.py      # Run a single test file\n\
                uv run pytest tests/test_foo.py::test_bar  # Run a single test\n\
                uv run ruff check .                  # Lint\n\
                uv run ruff format .                 # Format\n\
                uv run mypy .                        # Type check\n\
                ```\n\n\
                **General:**\n\
                ```bash\n\
                uv sync                              # Install/sync dependencies\n\
                uv run pre-commit run --all-files    # Run all pre-commit hooks\n\
                ```\n\n\
                ---\n\n\
                ## Code Style Guidelines\n\n\
                - **Language**: Prefer Python unless another language is clearly appropriate.\n\
                - **Formatting**: Use `ruff format` (88-char line length). Never hand-format what a tool can do.\n\
                - **Linting**: `ruff check` with auto-fix (`--fix`) where safe.\n\
                - **Types**: Annotate all function signatures. Use `mypy` for type checking.\n\
                - **Naming**: `snake_case` for functions/variables, `PascalCase` for classes, `UPPER_SNAKE` for constants.\n\
                - **Imports**: stdlib → third-party → local, separated by blank lines. Absolute imports only.\n\
                - **Error handling**: Raise specific exceptions; never bare `except:`. Log at the call site or let it propagate — not both.\n\
                - **Docstrings**: Google-style for public functions/classes.\n\n\
                ---\n\n\
                ## Issue Tracking with bd (beads)\n\n\
                **IMPORTANT**: Use **bd** for ALL task tracking. Do NOT use markdown TODOs or external trackers.\n\n\
                **Quick Reference:**\n\
                ```bash\n\
                bd ready                              # Find available (unblocked) work\n\
                bd ready --json                       # Machine-readable output\n\
                bd show <id>                          # View issue details\n\
                bd update <id> --claim --json         # Claim work atomically\n\
                bd close <id> --reason \"Done\" --json  # Complete work\n\
                bd sync                               # Sync with git\n\
                ```\n\n\
                **Create issues:**\n\
                ```bash\n\
                bd create \"Title\" --description=\"Context\" -t bug|feature|task|epic|chore -p 0-4 --json\n\
                bd create \"Found bug\" --description=\"Details\" -p 1 --deps discovered-from:<parent-id> --json\n\
                ```\n\n\
                ### Issue Types\n\
                - `bug` — Something broken\n\
                - `feature` — New functionality\n\
                - `task` — Tests, docs, refactoring\n\
                - `epic` — Large feature with subtasks\n\
                - `chore` — Maintenance (deps, tooling)\n\n\
                ### Priorities\n\
                - `0` — Critical (security, data loss, broken builds)\n\
                - `1` — High (major features, important bugs)\n\
                - `2` — Medium (default)\n\
                - `3` — Low (polish, optimization)\n\
                - `4` — Backlog (future ideas)\n\n\
                ### Workflow\n\
                1. `bd ready` — find unblocked issues\n\
                2. `bd update <id> --claim` — claim atomically\n\
                3. Implement, test, document\n\
                4. Discovered new work? `bd create \"...\" --deps discovered-from:<id>`\n\
                5. `bd close <id> --reason \"Done\"`\n\n\
                ### Rules\n\
                - Always use `--json` for programmatic/agent use\n\
                - Link discovered work with `discovered-from` dependencies\n\
                - Auto-sync: `.beads/issues.jsonl` exports after changes (5s debounce)\n\
                - Never create markdown TODO lists or duplicate tracking systems\n\n\
                ---\n\n\
                ## Non-Interactive Shell Commands\n\n\
                Shell aliases may add `-i` (interactive) flags, causing agents to hang. Always force non-interactive:\n\n\
                ```bash\n\
                cp -f source dest          # NOT: cp source dest\n\
                mv -f source dest          # NOT: mv source dest\n\
                rm -f file                 # NOT: rm file\n\
                rm -rf directory           # NOT: rm -r directory\n\
                cp -rf source dest         # NOT: cp -r source dest\n\
                ```\n\n\
                Other commands:\n\
                - `git log` / `git diff` — add `--no-pager`\n\
                - `git commit` — always `-m \"msg\"`, never bare\n\
                - `apt-get` — use `-y`\n\
                - `brew` — set `HOMEBREW_NO_AUTO_UPDATE=1`\n\n\
                ---\n\n\
                ## Landing the Plane (Session Completion)\n\n\
                Work is **NOT complete** until `git push` succeeds. Complete ALL steps:\n\n\
                1. **File issues** for any remaining or follow-up work\n\
                2. **Run quality gates** (tests, lint, type check) if code changed\n\
                3. **Update issue status** — close finished, update in-progress\n\
                4. **Push to remote:**\n\
                   ```bash\n\
                   git pull --rebase\n\
                   bd sync\n\
                   git push\n\
                   git status   # Must show \"up to date with origin\"\n\
                   ```\n\
                5. **Verify** — all changes committed AND pushed\n\
                6. **Hand off** — summarize context for next session\n\n\
                **NEVER** stop before pushing. **NEVER** say \"ready to push when you are\" — YOU must push.\n",
                layout_rows = layout_rows,
                pocket_dir = pocket_dir_str,
            ),
        )
        .context("Failed to create AGENTS.md")?;

        // ── .env (empty) ──────────────────────────────────────────────────────
        let env_file = self.pocket_dir.join(".env");
        fs::write(&env_file, "").context("Failed to create .env")?;

        // ── FEATURES/00.md ────────────────────────────────────────────────────
        let features_dir = self.pocket_dir.join("FEATURES");
        fs::create_dir_all(&features_dir).context("Failed to create FEATURES directory")?;

        let features_file = features_dir.join("00.md");
        fs::write(
            &features_file,
            "# Features\n\nAdd your feature ideas and documentation here.\n",
        )
        .context("Failed to create FEATURES/00.md")?;

        // ── observations/ ─────────────────────────────────────────────────────
        let observations_dir = self.pocket_dir.join("observations");
        fs::create_dir_all(&observations_dir).context("Failed to create observations directory")?;

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

    /// Read and parse a workspace file, returning the parsed struct and the extracted core paths.
    /// Filters out the pocket directory folder using path prefix matching.
    pub fn read_workspace_file(
        workspace_file: &Path,
        pocket_dir: &Path,
    ) -> Result<(VSCodeWorkspace, Vec<PathBuf>)> {
        let spocket_dir = Self::spocket_dir()?;

        let content =
            fs::read_to_string(workspace_file).context("Failed to read workspace file")?;

        let workspace: VSCodeWorkspace =
            serde_json::from_str(&content).context("Failed to parse workspace file")?;

        let core_paths: Vec<PathBuf> = workspace
            .folders
            .iter()
            .map(|f| PathBuf::from(&f.path))
            .filter(|p| !p.starts_with(pocket_dir) && !p.starts_with(&spocket_dir))
            .collect();

        Ok((workspace, core_paths))
    }

    /// Build a workspace file from core_paths, optionally preserving settings from an existing workspace.
    pub(crate) fn write_workspace_file_preserving(
        &self,
        existing: Option<&VSCodeWorkspace>,
    ) -> Result<()> {
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

        let workspace = if let Some(existing) = existing {
            VSCodeWorkspace {
                folders,
                settings: existing.settings.clone(),
                extensions: existing.extensions.clone(),
                launch: existing.launch.clone(),
                tasks: existing.tasks.clone(),
                extra: existing.extra.clone(),
            }
        } else {
            VSCodeWorkspace {
                folders,
                settings: None,
                extensions: None,
                launch: None,
                tasks: None,
                extra: serde_json::Map::new(),
            }
        };

        let workspace_json =
            serde_json::to_string_pretty(&workspace).context("Failed to serialize workspace")?;

        let workspace_path = self.workspace_file_path();
        fs::write(&workspace_path, workspace_json).context("Failed to write workspace file")?;

        Ok(())
    }

    pub fn create_workspace_file(&self) -> Result<()> {
        self.write_workspace_file_preserving(None)
    }

    fn init_git(&self) -> Result<()> {
        let output = Command::new("git")
            .args(["init"])
            .current_dir(&self.pocket_dir)
            .output()
            .context("Failed to execute git init")?;

        if !output.status.success() {
            return Err(anyhow!(
                "Git init failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        Ok(())
    }

    /// Set up Beads issue tracking in the safe pocket, and plant redirect stubs in every
    /// core project folder so `bd` commands work from the project directory.
    ///
    /// Idempotent: skips `bd init` if `.beads/` already exists in the pocket.
    pub fn setup_beads(&self) -> Result<()> {
        let beads_dir = self.pocket_dir.join(".beads");

        // ── 1. Run `bd init` inside the pocket directory (only if not already done) ──
        if beads_dir.exists() {
            println!(
                "{} {}",
                "Beads already initialised in pocket:".dimmed(),
                self.hash.bright_yellow()
            );
        } else {
            println!("{}", "Initialising Beads in safe pocket...".bright_white());

            let output = Command::new("bd")
                .args(["init", "--backend", "dolt"])
                .current_dir(&self.pocket_dir)
                .output()
                .context("Failed to execute `bd init` — is `bd` installed and on PATH?")?;

            if !output.status.success() {
                return Err(anyhow!(
                    "bd init failed:\n{}",
                    String::from_utf8_lossy(&output.stderr)
                ));
            }

            println!(
                "{} {}",
                "Beads initialised in:".bright_green(),
                beads_dir.display().to_string().bright_blue()
            );
        }

        // ── 2. Plant `.beads/redirect` stub in every core project folder ──────────
        let beads_dir_str = beads_dir.to_string_lossy().into_owned();

        for project_path in &self.core_paths {
            let project_beads_dir = project_path.join(".beads");
            let redirect_file = project_beads_dir.join("redirect");

            // Check if redirect already points to the right place (idempotent)
            if redirect_file.exists() {
                let existing = fs::read_to_string(&redirect_file).unwrap_or_default();
                if existing.trim() == beads_dir_str.trim() {
                    println!(
                        "{} {} {}",
                        "Redirect already set in:".dimmed(),
                        project_path.display().to_string().bright_blue(),
                        "(unchanged)".dimmed()
                    );
                    continue;
                }
            }

            fs::create_dir_all(&project_beads_dir).with_context(|| {
                format!(
                    "Failed to create .beads/ in project folder: {}",
                    project_path.display()
                )
            })?;

            fs::write(&redirect_file, &beads_dir_str).with_context(|| {
                format!(
                    "Failed to write .beads/redirect in: {}",
                    project_path.display()
                )
            })?;

            println!(
                "{} {} → {}",
                "Beads redirect planted in:".bright_green(),
                project_path.display().to_string().bright_blue(),
                beads_dir_str.bright_yellow()
            );
        }

        Ok(())
    }

    /// Detect workspace file drift and prompt the user to resolve it.
    pub fn detect_and_resolve_drift(&self) -> Result<DriftResult> {
        let workspace_path = self.workspace_file_path();

        if !workspace_path.exists() {
            return Ok(DriftResult::InSync);
        }

        let (workspace, file_paths) = Self::read_workspace_file(&workspace_path, &self.pocket_dir)?;

        // Compare with core paths
        let file_set: HashSet<_> = file_paths.iter().collect();
        let core_set: HashSet<_> = self.core_paths.iter().collect();

        if file_set == core_set {
            return Ok(DriftResult::InSync);
        }

        // Determine added and removed folders
        let added: Vec<_> = file_paths
            .iter()
            .filter(|p| !core_set.contains(p))
            .collect();
        let removed: Vec<_> = self
            .core_paths
            .iter()
            .filter(|p| !file_set.contains(p))
            .collect();

        println!("\n{}", "Workspace drift detected!".bright_yellow().bold());

        if !added.is_empty() {
            println!("  {} (in file but not in hash):", "Added".bright_green());
            for p in &added {
                println!("    + {}", p.display().to_string().bright_green());
            }
        }

        if !removed.is_empty() {
            println!("  {} (in hash but not in file):", "Removed".bright_red());
            for p in &removed {
                println!("    - {}", p.display().to_string().bright_red());
            }
        }

        println!();
        println!(
            "  {}  Accept workspace file as truth (migrate pocket)",
            "1.".bright_yellow()
        );
        println!(
            "  {}  Overwrite file to match original directories",
            "2.".bright_yellow()
        );
        println!("  {}  Do nothing", "3.".bright_yellow());
        println!();

        print!("{} ", "Choose [1/2/3]:".bright_white());
        use std::io::{self, Write as IoWrite};
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let input = input.trim();

        match input {
            "1" => Ok(DriftResult::AcceptFile {
                new_core_paths: file_paths,
            }),
            "2" => {
                self.write_workspace_file_preserving(Some(&workspace))?;
                println!(
                    "{}",
                    "Workspace file overwritten to match original directories.".bright_green()
                );
                Ok(DriftResult::OverwrittenFile)
            }
            _ => {
                println!("{}", "No changes made.".dimmed());
                Ok(DriftResult::Skipped)
            }
        }
    }

    pub fn open(&self) -> Result<()> {
        let workspace_path = self.workspace_file_path();

        if !workspace_path.exists() {
            return Err(anyhow!("Workspace file does not exist"));
        }

        // If there are sidecars, we need to temporarily add them
        if !self.sidecar_paths.is_empty() {
            println!(
                "Adding {} sidecar directories...",
                self.sidecar_paths.len().to_string().bright_yellow()
            );

            let (mut workspace, _) = Self::read_workspace_file(&workspace_path, &self.pocket_dir)?;

            // Add sidecars
            for path in &self.sidecar_paths {
                workspace.folders.insert(
                    0,
                    WorkspaceFolder {
                        path: path.to_string_lossy().to_string(),
                        name: Some(format!(
                            "[Sidecar] {}",
                            path.file_name().unwrap_or_default().to_string_lossy()
                        )),
                    },
                );
            }

            // Write temporarily
            let workspace_json = serde_json::to_string_pretty(&workspace)
                .context("Failed to serialize workspace")?;

            fs::write(&workspace_path, workspace_json).context("Failed to write workspace file")?;
        }

        println!("{}", "Opening workspace in VS Code...".bright_cyan());
        println!(
            "  {} {}",
            "File:".dimmed(),
            workspace_path.display().to_string().bright_blue()
        );

        // Open in VS Code
        let output = Command::new("code")
            .arg(&workspace_path)
            .output()
            .context("Failed to open VS Code")?;

        if !output.status.success() {
            return Err(anyhow!(
                "Failed to open VS Code: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        Ok(())
    }

    pub fn clone_from(source_path: &Path, target_paths: &[PathBuf]) -> Result<Self> {
        // Find workspace containing source_path
        let source_workspace = Self::find_workspace_containing(source_path)?;

        if source_workspace.is_none() {
            return Err(anyhow!(
                "No workspace found containing path: {}",
                source_path.display()
            ));
        }

        let source_workspace = source_workspace.unwrap();

        println!(
            "Cloning from workspace: {}",
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

        // Write manifest with lineage
        let manifest = Manifest::new_cloned(
            target_workspace.hash.clone(),
            target_workspace.core_paths.clone(),
            source_workspace.hash.clone(),
        );
        manifest.save(&target_workspace.pocket_dir)?;

        // Update parent's children list
        if let Ok(Some(mut parent_manifest)) = Manifest::load(&source_workspace.pocket_dir) {
            parent_manifest.add_child(target_workspace.hash.clone());
            let _ = parent_manifest.save(&source_workspace.pocket_dir);
        }

        println!(
            "{} {}",
            "Cloned to:".bright_green(),
            target_workspace.hash.bright_yellow()
        );

        Ok(target_workspace)
    }

    pub fn find_workspace_containing(path: &Path) -> Result<Option<Self>> {
        let spocket_dir = Self::spocket_dir()?;

        let entries = fs::read_dir(&spocket_dir).context("Failed to read .spocket directory")?;

        for entry in entries {
            let entry = entry?;
            let pocket_dir = entry.path();

            if !pocket_dir.is_dir() {
                continue;
            }

            // Use manifest-based lookup
            let (_, core_paths) = match Self::load_manifest_or_backfill(&pocket_dir)? {
                Some(result) => result,
                None => continue,
            };

            let hash = pocket_dir
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();

            // Check if any folder matches the path
            for cp in &core_paths {
                if cp == path {
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

    /// Find the workspace that "owns" the current directory.
    /// Checks if cwd is inside a pocket dir, or inside/equal to any workspace's core_paths.
    pub fn find_workspace_for_cwd(cwd: &Path) -> Result<Option<Self>> {
        let spocket_dir = Self::spocket_dir()?;

        // Check 1: Is CWD inside a ~/.spocket/<hash>/ directory?
        if cwd.starts_with(&spocket_dir) {
            if let Ok(relative) = cwd.strip_prefix(&spocket_dir) {
                if let Some(hash_component) = relative.components().next() {
                    let dir_name = hash_component.as_os_str().to_string_lossy().to_string();
                    let pocket_dir = spocket_dir.join(&dir_name);

                    if let Some((_, core_paths)) = Self::load_manifest_or_backfill(&pocket_dir)? {
                        return Ok(Some(Self {
                            hash: dir_name,
                            core_paths,
                            sidecar_paths: vec![],
                            pocket_dir,
                            create_readmes: false,
                        }));
                    }
                }
            }
        }

        // Check 2: Is CWD inside (or equal to) any workspace's core_paths?
        let entries = fs::read_dir(&spocket_dir).context("Failed to read .spocket directory")?;

        for entry in entries {
            let entry = entry?;
            let pocket_dir = entry.path();

            if !pocket_dir.is_dir() {
                continue;
            }

            let (_, core_paths) = match Self::load_manifest_or_backfill(&pocket_dir)? {
                Some(result) => result,
                None => continue,
            };

            let hash = pocket_dir
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();

            for cp in &core_paths {
                if cwd == cp || cwd.starts_with(cp) {
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

    /// Secondary lookup: scan all pocket manifests for one whose current `hash`
    /// matches `hash_paths(target_paths)`. Handles the case where paths evolved
    /// in-place (via sync or augment) and the directory name no longer matches.
    pub fn find_workspace_by_manifest_paths(target_paths: &[PathBuf]) -> Result<Option<Self>> {
        let target_hash = hash_paths(target_paths);
        let spocket_dir = Self::spocket_dir()?;

        let entries = fs::read_dir(&spocket_dir).context("Failed to read .spocket directory")?;

        for entry in entries {
            let entry = entry?;
            let pocket_dir = entry.path();

            if !pocket_dir.is_dir() {
                continue;
            }

            let manifest = match Manifest::load(&pocket_dir)? {
                Some(m) => m,
                None => continue,
            };

            if manifest.hash == target_hash {
                let dir_name = pocket_dir
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_string();

                return Ok(Some(Self {
                    hash: dir_name,
                    core_paths: manifest.core_paths,
                    sidecar_paths: vec![],
                    pocket_dir,
                    create_readmes: false,
                }));
            }
        }

        Ok(None)
    }

    pub fn list_all() -> Result<Vec<Self>> {
        let spocket_dir = Self::spocket_dir()?;

        let mut workspaces = Vec::new();

        let entries = fs::read_dir(&spocket_dir).context("Failed to read .spocket directory")?;

        for entry in entries {
            let entry = entry?;
            let pocket_dir = entry.path();

            if !pocket_dir.is_dir() {
                continue;
            }

            // Require a workspace file to exist
            if Self::find_workspace_file(&pocket_dir).is_none() {
                continue;
            }

            let (_, core_paths) = match Self::load_manifest_or_backfill(&pocket_dir)? {
                Some(result) => result,
                None => continue,
            };

            let hash = pocket_dir
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();

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
    pub fn find_similar_workspaces(
        target_paths: &[PathBuf],
        min_similarity: f64,
    ) -> Result<Vec<(Self, f64)>> {
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
        println!(
            "{}",
            "These workspaces share directories with your new workspace:".dimmed()
        );
        println!();

        for (i, (workspace, similarity)) in candidates.iter().enumerate() {
            let percentage = (similarity * 100.0) as u32;
            println!(
                "  {}. {} {}% similarity",
                (i + 1).to_string().bright_yellow(),
                workspace.hash.bright_blue(),
                percentage.to_string().bright_green()
            );

            // Show shared directories
            for path in &workspace.core_paths {
                println!("     - {}", path.display().to_string().dimmed());
            }
            println!();
        }

        println!(
            "  {}. {}",
            "0".bright_yellow(),
            "Don't clone (create fresh workspace)".dimmed()
        );
        println!();

        print!(
            "{} ",
            "Select a workspace to clone from (0-{}, or press Enter to skip):".bright_white()
        );
        use std::io::{self, Write as IoWrite};
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

        println!(
            "{}",
            "Invalid selection, creating fresh workspace".bright_yellow()
        );
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
        let paths1 = vec![PathBuf::from("/path/a"), PathBuf::from("/path/b")];
        let paths2 = vec![PathBuf::from("/path/b"), PathBuf::from("/path/a")];

        let similarity = Workspace::calculate_similarity(&paths1, &paths2);
        assert_eq!(similarity, 1.0);
    }

    #[test]
    fn test_calculate_similarity_partial() {
        let paths1 = vec![PathBuf::from("/path/a"), PathBuf::from("/path/b")];
        let paths2 = vec![PathBuf::from("/path/a"), PathBuf::from("/path/c")];

        let similarity = Workspace::calculate_similarity(&paths1, &paths2);
        // 1 common out of 3 total = 1/3 ≈ 0.33
        assert!((similarity - 0.333).abs() < 0.01);
    }

    #[test]
    fn test_calculate_similarity_no_overlap() {
        let paths1 = vec![PathBuf::from("/path/a")];
        let paths2 = vec![PathBuf::from("/path/b")];

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
        let paths2 = vec![PathBuf::from("/path/a"), PathBuf::from("/path/b")];

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

    #[test]
    fn test_vscode_workspace_preserves_extra_fields() {
        let json = r#"{
            "folders": [{"path": "/test"}],
            "settings": {"editor.fontSize": 16},
            "extensions": {"recommendations": ["rust-lang.rust-analyzer"]},
            "launch": {"version": "0.2.0"},
            "tasks": {"version": "2.0.0"},
            "customField": "preserved"
        }"#;

        let ws: VSCodeWorkspace = serde_json::from_str(json).unwrap();
        assert!(ws.settings.is_some());
        assert!(ws.extensions.is_some());
        assert!(ws.launch.is_some());
        assert!(ws.tasks.is_some());
        assert!(ws.extra.contains_key("customField"));

        // Round-trip
        let serialized = serde_json::to_string_pretty(&ws).unwrap();
        assert!(serialized.contains("customField"));
        assert!(serialized.contains("editor.fontSize"));
    }
}
