---
description: "Start or continue the advisory LumenHorizon collaborative chunk workflow for a chunk like 6.1."
name: "LumenHorizon Chunk"
argument-hint: "chunk x.y, next step, [Ask]/[Details] question, or update the doc"
agent: "agent"
---
Follow the LumenHorizon chunk workflow instructions in [LumenHorizon Chunk Workflow](../instructions/lumenhorizon-chunk-workflow.instructions.md).

This slash command is advisory by default. For `chunk x.y`, `the next step`, `[Ask]`, and `[Details]`, do not modify files or run commands that change repository, local service, database, or cloud state. Provide only the requested workflow summary, next instruction, or answer. Modify files only when the user explicitly asks to implement, apply edits, run verification, or update documentation.

Use the user's prompt arguments as the workflow request. If the request names a chunk, start that chunk by summarizing the small steps we will do together. If the request says `the next step`, continue with exactly one next actionable step. If the request starts with `[Ask]` or `[Details]`, answer the question without advancing the workflow. If the request says `update the doc`, update the relevant documentation for the completed chunk.

## Examples

Start a chunk:

```text
/lumenhorizon-chunk chunk 6.1
```

Continue with the next implementation step:

```text
/lumenhorizon-chunk the next step
```

Ask a detail question without moving forward:

```text
/lumenhorizon-chunk [Ask] What contract are we preserving with this migration?
```

Ask for details without moving forward:

```text
/lumenhorizon-chunk [Details] Why is this the next step?
```

Update documentation after the implementation is done:

```text
/lumenhorizon-chunk update the doc
```
