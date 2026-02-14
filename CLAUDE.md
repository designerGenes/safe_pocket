# Claude instructions — safe_pocket

Purpose
- Define strict rules for any Claude-generated or Claude-assisted code/edits in this repository.

Scope
- Applies to all Claude agents, prompts, suggested changes, and automated edits affecting this repository: `/Users/jadennation/DEV/bin/safe_pocket`.

Mandatory rules

1) Absolute paths only
- Always use full, absolute filesystem paths. Never use relative paths (no `.` or `..`, no `./`, no `../`).
- Example: use `/Users/jadennation/DEV/bin/safe_pocket/FEATURES/00.md` instead of `FEATURES/00.md` or `./FEATURES/00.md`.

2) Python: manage environments & deps with UV
- If Python is required, use **UV** to create/activate environments and to install/manage dependencies.
- Do not use `venv`, `virtualenv`, `pyenv`, `conda`, or the system Python for environment/dependency management in this repo.
- Document or script `UV` environment steps where needed and reference any files or environments by absolute path.

3) No Markdown files or summaries unless requested
- Do not create new `.md` files, update existing markdown, or produce project summaries unless explicitly asked by a human maintainer.
- If documentation or a summary is required to complete work, stop and request explicit permission before creating or modifying any markdown.

Enforcement & behavior
- If a proposed change violates any rule, do not apply it. Instead add a comment explaining which rule would be broken and ask for user confirmation.
- Always include the absolute path(s) in any message, patch, or code snippet that references repository files.

PR / agent checklist (must pass before applying changes)
- [ ] All file/directory references use absolute paths.
- [ ] Any Python work uses UV for envs/dependencies or is marked N/A.
- [ ] No markdown was created/modified without explicit user approval.

Location
- This instruction file: `/Users/jadennation/DEV/bin/safe_pocket/CLAUDE-instructions.md`

Date
- Created: 2026-02-14
