# Task Observer

Devlish implementation of the "One Skill to Rule Them All" meta-skill
(DEVL-79), based on [rebelytics/one-skill-to-rule-them-all](https://github.com/rebelytics/one-skill-to-rule-them-all).

## What it does

Monitors work sessions for skill improvement opportunities. Logs structured
observations, manages cross-cutting principles, and archives resolved entries.

## Programs

| Program | Purpose |
|---------|---------|
| `log_observation.dvl` | Append a new observation with auto-incrementing ID |
| `list_observations.dvl` | Return all OPEN observations as JSON |
| `archive_observations.dvl` | Move ACTIONED/DECLINED entries to dated archive |
| `manage_principles.dvl` | Add or list cross-cutting principles |

## MCP Tools

All programs are exposed as MCP tools via `devlish.toml`. Register with:

```bash
bin/devlish mcp  # from the task_observer directory
```

## Running manually

```bash
# Log an observation
bin/devlish run examples/task_observer/log_observation.dvl --input '{
  "workspace": "/tmp/skill-observations",
  "title": "Missing enforcement in skill-creator",
  "session_context": "Building a documentation skill",
  "skill": "skill-creator",
  "obs_type": "open-source",
  "phase": "Pre-flight checklist",
  "issue": "Skill-creator does not verify output against its own rules",
  "suggested_improvement": "Add a verification step",
  "principle": "Every skill with rules should include enforcement"
}'

# List open observations
bin/devlish run examples/task_observer/list_observations.dvl \
  --input '{"workspace": "/tmp/skill-observations"}'

# Add a cross-cutting principle
bin/devlish run examples/task_observer/manage_principles.dvl --input '{
  "workspace": "/tmp/skill-observations",
  "action": "add",
  "title": "Built-in enforcement",
  "applies_to": "all skills with rules",
  "requirement": "Every skill that contains rules must include a verification step",
  "propagation": "immediate"
}'

# List active principles
bin/devlish run examples/task_observer/manage_principles.dvl \
  --input '{"workspace": "/tmp/skill-observations", "action": "list"}'

# Archive resolved observations
bin/devlish run examples/task_observer/archive_observations.dvl \
  --input '{"workspace": "/tmp/skill-observations"}'
```

## Features exercised

- Filesystem keywords: `Check if exists`, `Create directory`, `Read text from`
- String operations: `split`, `replace`, `contains`, `trim`
- Program manifest: `Permissions:` header
- Structured output: `Respond with` for JSON results
- String escapes: `\n` for multi-line file content
- MCP tool discovery via `devlish.toml`
