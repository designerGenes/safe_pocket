# Safe Pocket (spocket)

Safe Pocket is a CLI tool for managing ad hoc VS Code workspaces based on directory combinations, enabling persistent AI copilot customization across different project contexts.

## The Problem

When working across multiple projects or features in a monorepo, you often need:
- Custom Copilot instructions specific to your workflow
- Context-aware AI assistance without bloating the main codebase
- Ability to combine directories from different projects into one workspace
- Persistent customization that doesn't require approval from repository maintainers

Traditional approaches fail because:
- Workspace names don't work well for combinations of multiple projects
- Custom copilot instructions are hard to get merged into shared codebases
- You lose your customizations when switching between features

## The Solution

Safe Pocket creates "safe pockets" - version-controlled directories that contain:
- Custom Copilot instructions (`.github/copilot-instructions.md`)
- Feature documentation (`FEATURES/`)
- Observations and learnings (`observations/`)
- VS Code workspace configuration

Each safe pocket is identified by a hash of the directories it contains, making it deterministic and reusable.

## Installation

### From Source

```bash
git clone <repository-url>
cd safe_pocket
cargo build --release
```

The binary will be at `target/release/spocket`.

### Add to PATH

```bash
# Copy to a location in your PATH
cp target/release/spocket /usr/local/bin/

# Or create an alias in your shell config
echo 'alias spocket="/path/to/safe_pocket/target/release/spocket"' >> ~/.zshrc
```

## Usage

### Register Directory Aliases

Register frequently-used directories with memorable names:

```bash
spocket register myproject="$HOME/dev/myproject"
spocket register backend="$HOME/dev/api"
```

### List Aliases

```bash
spocket list
```

### Create/Open Workspaces

Create a workspace from one or more directories:

```bash
# Using an alias
spocket -i myproject

# Using multiple directories
spocket -i myproject -i backend

# Using full paths
spocket -i ~/dev/project1 -i ~/dev/project2

# Using shell variables (expand before passing)
devBin="$HOME/dev/bin"
spocket -i $devBin -i ~/dev/personal
```

### Sidecar Directories

Add temporary directories that won't be saved to the workspace:

```bash
# Add ~/dev/tools as a sidecar (temporary)
spocket -i myproject -i backend --sidecar ~/dev/tools
```

Sidecars are useful for directories you need occasionally but don't want permanently in the workspace. They're added when opening but not saved to the `.code-workspace` file.

### Clone Safe Pockets

Copy safe pocket contents (copilot instructions, features, etc.) from one workspace to another:

```bash
# Clone from workspace containing myproject to a new workspace
spocket -i newproject --clone-from myproject
```

### Smart Cloning (Automatic)

When creating a new workspace, spocket automatically detects similar existing workspaces based on directory overlap. If a similar workspace is found (with at least 30% similarity), you'll be prompted to clone from it:

```bash
# Create workspace with dir1 and dir2
spocket -i dir1 -i dir2 -i dir3

# Later, create a similar workspace (dir1 and dir2 only)
spocket -i dir1 -i dir2

# Output:
# 🔍 Similar workspaces found!
# These workspaces share directories with your new workspace:
#
#   1. abc123def456 (66% similarity)
#      → /path/to/dir1
#      → /path/to/dir2
#      → /path/to/dir3
#
#   0. Don't clone (create fresh workspace)
#
# Select a workspace to clone from (0-1, or press Enter to skip):
```

This allows you to:
- Reuse customizations when working on related features
- Avoid manually tracking which workspaces have useful copilot instructions
- Quickly bootstrap new workspaces with proven configurations

**Similarity Calculation:** Spocket uses Jaccard similarity (intersection over union) to measure how similar workspace directories are. A workspace with 2 out of 3 directories matching has 66% similarity.

### List All Workspaces

```bash
spocket list-workspaces
```

### Unregister Aliases

```bash
spocket unregister myproject
```

### Skip README Generation

By default, spocket creates helpful README files in empty directories explaining their purpose. To skip these:

```bash
# Create workspace without READMEs
spocket -i myproject --no-readme
```

**Note:** READMEs are never created when cloning from an existing workspace (they're preserved from the source).

### Version

Check which version of spocket you have:

```bash
spocket -v
spocket --version
```

### Verbose Output

By default, informational messages (such as runtime merge notifications) are suppressed for a clean experience. Enable them when you want to see what's happening under the hood:

```bash
spocket -i myproject --verbose
```

## How It Works

1. **Hashing**: When you specify directories with `-i`, spocket sorts and hashes their full paths to create a unique 12-character identifier.

2. **Safe Pocket Creation**: A directory is created at `~/.safe_pocket/<hash>/` containing:
   - `.github/prompts/` - Custom prompt templates
   - `.github/copilot-instructions.md` - Copilot instructions
   - `FEATURES/00.md` - Feature documentation
   - `observations/` - AI-generated insights
   - `<hash>.code-workspace` - VS Code workspace file
   - `.git/` - Git repository for version control

3. **Workspace Structure**: The workspace includes:
   - All your specified directories
   - The safe pocket directory itself

4. **Mismatch Detection**: If the workspace file contains different directories than the hash suggests, spocket warns you.

## Directory Structure

```
~/.safe_pocket/
└── abc123def456/              # Hash of included directories
    ├── .git/                  # Git repository
    ├── .github/
    │   ├── prompts/
    │   └── copilot-instructions.md
    ├── FEATURES/
    │   └── 00.md
    ├── observations/
    └── abc123def456.code-workspace

~/.safe_pocket/
└── registry/
    └── aliases.json           # Alias registry
```

## Configuration

Configuration is stored at `~/.safe_pocket/registry/aliases.json`:

```json
{
  "aliases": {
    "myproject": "/Users/username/dev/myproject",
    "backend": "/Users/username/dev/backend"
  }
}
```

## Examples

### Monorepo Feature Development

```bash
# Work on a specific feature with custom copilot instructions
spocket -i ~/monorepo/features/auth

# Add observability tools as a sidecar
spocket -i ~/monorepo/features/auth --sidecar ~/dev/tools
```

### Multi-Project Workspace

```bash
# Combine iOS project with backend API
spocket -i ~/projects/ios-app -i ~/projects/backend-api

# Your custom copilot instructions work across both projects
```

### Reusing Customizations

```bash
# Created great copilot instructions for feature-a
spocket -i ~/monorepo/feature-a

# Clone them to feature-b
spocket -i ~/monorepo/feature-b --clone-from ~/monorepo/feature-a
```

## Recent Features (v0.3.0)

- ✅ **`-v`/`--version`**: Check the installed version with `spocket -v` or `spocket --version`
- ✅ **`--verbose` flag**: Opt-in to detailed output (runtime merge notices, template installs, etc.)
- ✅ **Cleaner default output**: Non-error, non-warning messages are now hidden by default

### Previous: v0.2.1

- ✅ **No emojis**: Cleaner, more professional CLI output
- ✅ **README files** in safe pocket directories explaining their purpose
- ✅ **`--no-readme` flag** to skip README generation

### Previous: v0.2.0

- ✅ **Smart Cloning**: Automatic detection of similar workspaces with interactive selection
- ✅ **Similarity Calculation**: Jaccard-based similarity scoring for workspace matching
- ✅ **Comprehensive Testing**: Unit tests for core functionality

## Future Features

- GitHub repo integration for safe pockets (push/pull safe pocket as a repo)
- Workspace editing without breaking hash associations (add/remove directories)
- Custom templates for safe pocket contents
- Beads integration (Ralph Wiggum loops and custom API interactions)
- Workspace cleanup utilities (remove unused workspaces, fix mismatches)
- Configurable similarity threshold for smart cloning
- Workspace tagging and search
- Export/import safe pocket configurations

## Contributing

This project is in early development. Feedback and contributions are welcome!

## License

[Specify your license here]
