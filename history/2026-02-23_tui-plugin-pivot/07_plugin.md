# Phase 6: Plugin Files

Parent: [00_overview.md](00_overview.md)
Depends on: [06_cli-subcommands.md](06_cli-subcommands.md)

## Goal

Create the Claude Code plugin: plugin.json, skills, and agents.

## Plugin Structure

```
.claude-plugin/
  plugin.json
skills/
  recall/SKILL.md
  explain-session/SKILL.md
agents/
  knowledge-miner.md
```

Plugin files live at repo root.

## plugin.json

```json
{
  "name": "mementor",
  "version": "2.0.0",
  "description": "TUI workspace tool and knowledge-mining skills for Claude Code. Provides /recall for cross-session knowledge search and /explain-session for checkpoint deep-dives.",
  "author": "fenv-org",
  "repository": "https://github.com/fenv-org/mementor"
}
```

## /recall Skill

```yaml
---
name: recall
description: >
  Search past coding sessions for relevant knowledge and context.
  Use when you need to recall what was done before, find related
  past work, or understand the history behind code changes.
disable-model-invocation: false
user-invocable: true
argument-hint: "<what to search for>"
context: fork
agent: knowledge-miner
allowed-tools: Bash(mementor *), Bash(entire *), Bash(git log *), Read, Glob, Grep
---
```

**`context: fork`** runs the skill in a separate subagent context using the
`knowledge-miner` agent:
- knowledge-miner handles search autonomously
- Results returned as summary to main agent
- Main agent's context window NOT consumed by transcript data

**Search strategy** (skill body):

1. **Check active sessions first**: Run `entire status`. If current session
   compacted, search live transcript for lost context.
2. **Quick metadata scan**: `mementor list` for all checkpoints.
3. **Identify candidates**: Filter by files_touched, branch, time, keywords.
4. **Deep read**: For top 3 candidates, `mementor transcript <id>`.
5. **Synthesize**: Key decisions, code patterns, source checkpoints.
6. **If `entire` available**: `entire explain --checkpoint <id> --short`.

## /explain-session Skill

```yaml
---
name: explain-session
description: >
  Explain what happened in a specific coding session, commit, or
  checkpoint.
disable-model-invocation: false
user-invocable: true
argument-hint: "<session-id, commit-hash, or checkpoint-id>"
allowed-tools: Bash(mementor *), Bash(entire *), Bash(git *)
---
```

**Body**:
1. Determine input type (UUID / commit hash / 12-char checkpoint ID)
2. For commits: `entire explain --commit <sha> --no-pager`
3. For checkpoints: `mementor show <id>` + `mementor transcript <id>`
4. For session IDs: search checkpoints for matching session
5. Present: what was done, why, key decisions, files changed, outcomes

## knowledge-miner Agent

```yaml
---
name: knowledge-miner
description: >
  Autonomous researcher that searches through past coding session
  history across multiple dimensions.
tools: Bash, Read, Glob, Grep
model: inherit
maxTurns: 15
---
```

**`model: inherit`** uses parent agent's model selection.

**Research strategy**:

1. **Analyze question** — topics, file paths, terms, time refs, PR numbers
2. **Multi-angle investigation** (3+ angles):
   - `mementor list` → filter by files_touched, branch, time
   - `mementor search "<terms>"` → keyword search
   - `entire explain --checkpoint <id> --short` → AI summaries
   - `git log --oneline --grep="<term>"` → commit message search
   - `mementor files <id>` → file-level investigation
3. **Deep dive**: `mementor transcript <id>`, `entire explain --checkpoint <id>`
4. **Cross-reference**: Trace pattern evolution across sessions
5. **Return**: Key results, source checkpoints, timeline, confidence

## TODO

- [ ] Create `.claude-plugin/plugin.json`
- [ ] Write `skills/recall/SKILL.md` with search strategy body
- [ ] Write `skills/explain-session/SKILL.md`
- [ ] Write `agents/knowledge-miner.md` with research strategy
- [ ] Test skills work with `claude -p` or manual invocation
