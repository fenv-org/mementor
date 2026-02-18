# Claude Code Transcript JSONL Format

Claude Code stores conversation history as JSONL (JSON Lines) files. Each line
is a self-contained JSON object representing one event in the conversation.

## Common Fields

Most entry types share these fields:

| Field | Type | Description |
|-------|------|-------------|
| `type` | string | Entry type discriminator |
| `uuid` | string | Unique entry identifier |
| `sessionId` | string | Session this entry belongs to |
| `timestamp` | string | ISO 8601 timestamp |
| `parentUuid` | string? | Parent entry UUID (conversation tree) |
| `cwd` | string? | Working directory at time of entry |
| `version` | string? | Claude Code version |
| `gitBranch` | string? | Active git branch |
| `isSidechain` | bool? | true for compaction agent entries |
| `userType` | string? | e.g., `"external"` |

## Entry Types

### `user`

A user message (prompt, tool result, or slash command).

| Field | Type | Description |
|-------|------|-------------|
| `message.role` | string | Always `"user"` |
| `message.content` | string \| ContentBlock[] | Plain text or array of blocks |
| `isMeta` | bool? | Metadata-only entry (e.g., local command caveat) |
| `thinkingMetadata` | object? | `{ maxThinkingTokens: number }` |
| `permissionMode` | string? | e.g., `"plan"`, `"auto"` |
| `sourceToolAssistantUUID` | string? | UUID of assistant that triggered tool |
| `toolUseResult` | object? | Result from tool execution |

Content block types in user messages:

- `{ "type": "text", "text": "..." }` — user's typed text
- `{ "type": "tool_result", "tool_use_id": "...", "content": "...", "is_error": bool }` — tool output

**Example (plain text):**

```json
{
  "type": "user",
  "uuid": "365bbb81-...",
  "sessionId": "0dc751f2-...",
  "timestamp": "2026-02-17T12:14:10.743Z",
  "message": {
    "role": "user",
    "content": "How do I fix the CI?"
  },
  "parentUuid": null,
  "cwd": "/Users/user/project",
  "version": "2.1.44",
  "gitBranch": "main",
  "isSidechain": false,
  "userType": "external"
}
```

**Example (tool result — content blocks array):**

```json
{
  "type": "user",
  "uuid": "60687c6b-...",
  "sessionId": "0dc751f2-...",
  "timestamp": "2026-02-17T12:43:06.280Z",
  "message": {
    "role": "user",
    "content": [
      {
        "type": "tool_result",
        "tool_use_id": "toolu_01Tr67...",
        "content": "file content here...",
        "is_error": false
      }
    ]
  },
  "parentUuid": "f1f702a6-...",
  "sourceToolAssistantUUID": "f1f702a6-...",
  "toolUseResult": {
    "type": "text",
    "file": {
      "filePath": "/path/to/file.rs",
      "content": "...",
      "numLines": 34,
      "startLine": 1,
      "totalLines": 34
    }
  }
}
```

---

### `assistant`

An assistant response (text, thinking, tool calls).

| Field | Type | Description |
|-------|------|-------------|
| `message.role` | string | Always `"assistant"` |
| `message.model` | string | Model ID (e.g., `"claude-opus-4-6"`) |
| `message.content` | ContentBlock[] | Array of content blocks (always array) |
| `message.stop_reason` | string? | `"end_turn"`, `"tool_use"`, or null |
| `message.usage` | object | Token usage breakdown |
| `requestId` | string | API request ID |
| `agentId` | string? | Set for compaction/subagent entries |

**Content block types in assistant messages:**

**Text block:**

```json
{ "type": "text", "text": "Here is the answer..." }
```

**Thinking block:**

```json
{
  "type": "thinking",
  "thinking": "I need to consider...",
  "signature": "EvoGCkYICxgC..."
}
```

The `thinking` field contains the model's internal reasoning. The `signature`
is a cryptographic signature for cache verification (discard for RAG purposes).

**Tool use block:**

```json
{
  "type": "tool_use",
  "id": "toolu_01G7qT...",
  "name": "Bash",
  "input": {
    "command": "cargo test",
    "description": "Run tests"
  }
}
```

**Tool use `input` field structure by tool name:**

| Tool | Key Fields |
|------|------------|
| Read | `{ "file_path": "/path/to/file" }` |
| Edit | `{ "file_path": "/path/to/file", "old_string": "...", "new_string": "..." }` |
| Write | `{ "file_path": "/path/to/file", "content": "..." }` |
| Bash | `{ "command": "...", "description": "..." }` |
| Grep | `{ "pattern": "...", "path": "..." }` |
| Glob | `{ "pattern": "..." }` |
| Task | `{ "prompt": "...", "description": "..." }` |

**Example (with thinking, text, and tool use):**

```json
{
  "type": "assistant",
  "uuid": "d9c2a659-...",
  "sessionId": "0dc751f2-...",
  "timestamp": "2026-02-17T12:14:19.783Z",
  "message": {
    "model": "claude-opus-4-6",
    "id": "msg_01KEFi...",
    "type": "message",
    "role": "assistant",
    "content": [
      {
        "type": "thinking",
        "thinking": "The user wants to fix CI...",
        "signature": "EvoGCkYICxgC..."
      },
      {
        "type": "text",
        "text": "I'll update the workflow file."
      },
      {
        "type": "tool_use",
        "id": "toolu_01G7qT...",
        "name": "Edit",
        "input": {
          "file_path": "/project/.github/workflows/ci.yml",
          "old_string": "runs-on: ubuntu-latest",
          "new_string": "runs-on: ubuntu-22.04"
        }
      }
    ],
    "stop_reason": "tool_use",
    "usage": {
      "input_tokens": 3000,
      "output_tokens": 200,
      "cache_creation_input_tokens": 9885,
      "cache_read_input_tokens": 18058
    }
  },
  "requestId": "req_011CY..."
}
```

---

### `system`

System metadata entries with subtypes.

| `subtype` | Description |
|-----------|-------------|
| `compact_boundary` | Marks where compaction occurred |
| `local_command` | Slash command execution (e.g., `/status`) |
| `stop_hook_summary` | Hook execution summary after stop |
| `turn_duration` | Turn timing information |

**`compact_boundary` subtype (important for mementor):**

```json
{
  "type": "system",
  "subtype": "compact_boundary",
  "content": "Conversation compacted",
  "level": "info",
  "uuid": "712d09ee-...",
  "sessionId": "59c98839-...",
  "timestamp": "2026-02-09T12:19:47.557Z",
  "logicalParentUuid": "3a3b8268-...",
  "compactMetadata": {
    "trigger": "auto",
    "preTokens": 170551
  }
}
```

Key fields:
- `compactMetadata.trigger`: `"auto"` (context limit) or `"manual"` (user `/compact`)
- `compactMetadata.preTokens`: token count before compaction

**`turn_duration` subtype:**

```json
{
  "type": "system",
  "subtype": "turn_duration",
  "durationMs": 112990
}
```

---

### `pr-link`

Links a session to a GitHub pull request. Has **NO `message` field**.

| Field | Type | Description |
|-------|------|-------------|
| `prNumber` | number | PR number |
| `prUrl` | string | Full PR URL |
| `prRepository` | string | `"owner/repo"` format |

**Example:**

```json
{
  "type": "pr-link",
  "sessionId": "0dc751f2-...",
  "prNumber": 14,
  "prUrl": "https://github.com/fenv-org/mementor/pull/14",
  "prRepository": "fenv-org/mementor",
  "timestamp": "2026-02-17T13:23:30.089Z"
}
```

---

### `progress`

Streaming progress events (tool execution, hooks, subagents). High-volume
noise — typically 42-73% of all transcript lines.

| Field | Type | Description |
|-------|------|-------------|
| `data.type` | string | `"hook_progress"`, `"agent_progress"`, etc. |
| `data.agentId` | string? | Subagent identifier |
| `data.hookEvent` | string? | Hook event name (e.g., `"SessionStart"`) |
| `toolUseID` | string? | Associated tool use |
| `parentToolUseID` | string? | Parent tool use ID |

**Example (hook progress):**

```json
{
  "type": "progress",
  "data": {
    "type": "hook_progress",
    "hookEvent": "SessionStart",
    "hookName": "SessionStart:startup",
    "command": "bash scripts/setup.sh"
  },
  "uuid": "13e64c6f-...",
  "sessionId": "3108a700-...",
  "timestamp": "2026-02-14T03:42:53.676Z"
}
```

**Example (agent progress):**

```json
{
  "type": "progress",
  "data": {
    "type": "agent_progress",
    "prompt": "Explore the project...",
    "agentId": "a9a1190"
  },
  "toolUseID": "agent_msg_01KEFi...",
  "parentToolUseID": "toolu_016biP..."
}
```

---

### `file-history-snapshot`

File backup snapshots for undo functionality.

| Field | Type | Description |
|-------|------|-------------|
| `messageId` | string | Associated message UUID |
| `snapshot.trackedFileBackups` | object | Map of file paths to backups |
| `snapshot.timestamp` | string | ISO 8601 timestamp |
| `isSnapshotUpdate` | bool | Whether this updates a previous snapshot |

**Example:**

```json
{
  "type": "file-history-snapshot",
  "messageId": "15297d8d-...",
  "snapshot": {
    "messageId": "15297d8d-...",
    "trackedFileBackups": {},
    "timestamp": "2026-02-14T03:49:27.861Z"
  },
  "isSnapshotUpdate": false
}
```

---

### `queue-operation`

Queue management events.

| Field | Type | Description |
|-------|------|-------------|
| `operation` | string | e.g., `"dequeue"` |

**Example:**

```json
{
  "type": "queue-operation",
  "operation": "dequeue",
  "timestamp": "2026-02-07T07:42:54.402Z",
  "sessionId": "78e7f3d0-..."
}
```

---

### `summary`

Compaction summary entries generated by the compaction agent. These contain
condensed versions of the conversation before compaction.

---

## Volume Distribution (from real transcripts)

| Entry Type | Typical % | Mementor Relevance |
|------------|-----------|-------------------|
| `progress` | 42-73% | Skip (streaming noise) |
| `user` | 8-15% | **Index** (prompts, tool results) |
| `assistant` | 8-15% | **Index** (responses, thinking, tool calls) |
| `system` | 5-10% | Track `compact_boundary` only |
| `file-history-snapshot` | 3-8% | Skip |
| `pr-link` | <1% | **Store** in `pr_links` table |
| `queue-operation` | <1% | Skip |
| `summary` | <1% | **Index** (compaction summaries) |
