#SPOCKET_TEMPLATE_DESTINATION: .github/copilot-instructions.md
# Project folder versus safe pocket folder

This file is contained inside a subdirectory of
{{SPOCKET_ROOT}}

That folder is a "safe pocket" folder. It is NOT the "project folder". The safe pocket folder contains "meta files" which are relevant only to the actual project folder. The actual project folder is
{{PROJECT_ROOT}}

All commands made to the agent are intended to be applied to the project folder, not the safe pocket folder. The safe pocket folder is only for storing meta files that are relevant to the project folder. The agent should never make any changes to the safe pocket folder, only read from it.

For example, if the agent is asked to review our codebase, it should read the code files from the project folder, not the safe pocket folder. The safe pocket folder may contain instructions or other meta files that are relevant to the project folder, but the actual code files are in the project folder.

# Rules

1. You must always use full paths whenever you reference any file or directory. NEVER use relative paths.
2. If instructed to use a "cli app" or "terminal command", you should run this command in the context of the project folder, not the safe pocket folder. You should also always try to run the literal command you are told to use, before searching for python files or source code. For example, if I tell you "use the cli app sponge_bob to do X", you must first attempt to run the command "sponge_bob" in the terminal, and only if that fails should you search for a python file or source code that might be relevant.
3. All python dependencies and environments are managed by 'uv', never by 'pip'.

# Observations Logging

As you work, you will inevitably discover significant insights about the project, codebase, patterns, bugs, conventions, and other noteworthy findings. You are required to actively log these as "observation" files in the safe pocket folder.

## What qualifies as an Observation

Log an observation whenever you discover any of the following:
- Architectural patterns or design decisions in the codebase
- Recurring bugs, anti-patterns, or footguns
- Non-obvious conventions or project-specific idioms
- Important constraints (e.g., dependency quirks, environment limitations)
- Useful techniques or shortcuts specific to this project
- Surprising or counter-intuitive behavior you encounter
- Decisions made during a session that future sessions should know about

When in doubt, log it. Observations are cheap to create and valuable to retain.

## Where to write Observations

Always write observation files to:
```
{{SPOCKET_ROOT}}/observations/
```

This is the safe pocket folder — writing here is explicitly permitted for observation logging.

## Naming Convention

Name each file using the following format:
```
YYYY-MM-DD--<slug>.md
```

Where `<slug>` is a short, lowercase, hyphen-separated summary of the observation's subject derived from its content. The slug should be specific enough to be meaningful at a glance.

Examples:
- `2025-06-10--uv-env-not-activated-by-default.md`
- `2025-06-10--project-uses-ruff-not-black.md`
- `2025-06-11--api-auth-token-stored-in-dotenv.md`

Do NOT use generic slugs like `observation-1` or `misc-finding`.

## File Format

Each observation file should be a short Markdown file with the following structure:

```markdown
# <Title of Observation>

**Date:** YYYY-MM-DD  
**Context:** <Brief description of what you were doing when you made this observation>

## Finding

<Clear, concise description of what you observed.>

## Why It Matters

<Why this is worth knowing for future sessions or contributors.>

## Notes

<Any additional details, caveats, or related links. Omit if not needed.>
```

Keep observations focused. One observation per file. Split large findings into multiple files if needed.
