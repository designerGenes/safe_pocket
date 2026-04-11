# Changelog

All notable changes to Safe Pocket (spocket) will be documented in this file.

## [0.3.0] - 2026-04-11

### Added
- **`-v`/`--version` flag** to display the application version (`spocket -v`)
- **`--verbose` flag** to enable detailed informational output

### Changed
- **Quieter default output**: Non-error, non-warning messages (e.g. "Runtime merged: ...", "Installed default template: ...") are now hidden unless `--verbose` is passed. This keeps the normal launch experience clean and uncluttered.

## [0.2.1] - 2026-02-14

### Changed
- **Removed all emojis** from user-facing output and VS Code workspace names
  - "[Safe Pocket]" instead of "🔒 Safe Pocket"
  - "[Sidecar]" instead of "🔗"
  - Cleaner, more professional CLI output
- **Simplified output** - removed decorative symbols from messages

### Added
- **README files** in safe pocket directories explaining their purpose
  - Root README explaining the safe pocket structure
  - `.github/prompts/README.md` - How to use prompt templates
  - `observations/README.md` - Purpose of the observations directory
- **`--no-readme` flag** to skip README generation if desired
- READMEs are automatically excluded when smart cloning (preserves source content)

## [0.2.0] - 2026-02-14

### Added - Features from 01.md

#### Smart Cloning
- **Automatic similarity detection**: When creating a new workspace, spocket now scans existing workspaces to find similar ones based on directory overlap
- **Interactive selection menu**: If similar workspaces are found (≥30% similarity), users are prompted to choose which one to clone from
- **Jaccard similarity scoring**: Uses intersection-over-union to calculate workspace similarity (e.g., 2 matching dirs out of 3 total = 66% similarity)
- **Multiple candidate support**: Displays all similar workspaces ranked by similarity percentage

#### Testing Infrastructure
- Added comprehensive unit tests for similarity calculations
- Tests for identical, partial, subset, and no-overlap scenarios
- Tests for edge cases (empty paths, single paths)
- All 7 tests passing in test suite

#### Improvements
- Made `create_workspace_file()` public for better reusability
- Enhanced workspace creation flow with smart cloning integration
- Better error handling with Context trait from anyhow
- Improved user experience with clear prompts and colored output

### Technical Details

**New Functions in `workspace.rs`:**
- `calculate_similarity(paths1, paths2) -> f64`: Calculates Jaccard similarity between path sets
- `find_similar_workspaces(target_paths, min_similarity) -> Vec<(Workspace, f64)>`: Finds and ranks similar workspaces
- `prompt_clone_selection(candidates) -> Option<&Workspace>`: Interactive menu for workspace selection

**Modified Functions:**
- `handle_workspace()` in `main.rs`: Integrated smart cloning before workspace creation

### What This Solves

Smart cloning addresses the key problem described in FEATURES/01.md:

> "Maybe this can benefit from the contents of D1_D2_D3_SpocketDir, even though it does not contain D2. If a workspace like this, which 'looks like' our existing workspace, is created, our safe_pocket app should recognize this and ask at launch time if we want to 'clone' in the file contents from (likely) parent safe pocket."

**Real-world scenario:**
1. You create a workspace for `[feature-a, shared-lib, tools]` with custom copilot instructions
2. Later, you need to work on just `[feature-b, shared-lib]`
3. spocket detects 33% similarity (1 of 3 dirs match)
4. You're prompted to clone the copilot instructions from the first workspace
5. You get your customizations without manual copy-paste

### Migration Notes

No breaking changes. All existing functionality preserved.

## [0.1.0] - 2026-02-14

### Initial Release

- Hash-based workspace naming (SHA256, 12-char truncation)
- Alias registration and management
- Workspace creation with safe pocket structure
- Sidecar directory support (ephemeral additions)
- Mismatch detection and warnings
- Manual safe pocket cloning with `--clone-from`
- Git initialization for each safe pocket
- VS Code workspace generation and opening
- Colored CLI output
- Configuration management in `~/.config/spocket/`
- Safe pocket storage in `~/.spocket/`
