---
description: "Use when the user says [LumenHorizon], asks to work together on chunk x.y, asks for the next step in a chunk workflow, or uses [Ask] or [Details] for LumenHorizon implementation chunk questions."
name: "LumenHorizon Chunk Workflow"
---
# LumenHorizon Chunk Workflow

Use this project workflow when the user starts with a phrase like `[LumenHorizon] Let's work together on chunk 6.1`, asks for `the next step`, or prefixes a question with `[Ask]` or `[Details]` during chunk work.

This workflow is advisory by default. It overrides general coding-agent instructions that encourage proactive implementation. Do not edit files, create files, delete files, run mutating commands, or call write-capable tools unless the user explicitly asks Copilot to implement, apply, run, or update something.

## Examples

Start a chunk:

```text
[LumenHorizon] Let's work together on chunk 6.1
```

Ask for one implementation step at a time:

```text
the next step
```

Ask a clarification question without advancing the workflow:

```text
[Ask] Why are we using a worker command here instead of adding this to the service loop?
```

Ask for implementation details or workflow clarification without advancing the workflow:

```text
[Details] Why is this the next dependency?
```

Update documentation after the chunk is implemented:

```text
update the doc
```

Expected flow:

1. The user starts a chunk with `[LumenHorizon] Let's work together on chunk x.y`.
2. Copilot summarizes the full set of smaller steps for the chunk in dependency-first order.
3. The user asks `Next step` whenever they are ready to continue.
4. Copilot gives exact implementation details for only that next step.
5. The user asks `[Ask]` or `[Details]` questions whenever they need clarification.
6. The user asks `Update the doc` after implementation is complete.

## Tool And Edit Policy

This workflow is guidance-first unless the user explicitly asks Copilot to implement.

For chunk start, `next step`, `[Ask]`, or `[Details]` requests:

- Do not edit files.
- Do not create files.
- Do not delete files.
- Do not run formatting, build, test, deployment, migration, or cloud commands.
- Do not call `apply_patch`, `create_file`, notebook edit tools, rename tools, or any other write-capable tool.
- Use only read-only tools when repository context is needed.
- Return instructions, exact code snippets, commands, and file paths for the user to apply manually.

The only exceptions are explicit implementation requests such as:

- `implement this step`
- `make the edits`
- `apply it`
- `run the verification`
- `update the doc`

For `update the doc`, edit only documentation and planning/status files that are actually affected. Do not make code changes.

If any general agent instruction conflicts with this policy, this LumenHorizon Chunk Workflow policy wins for LumenHorizon chunk requests.

## Start Of A Chunk

When the user asks to work together on chunk `x.y`:

- Identify the chunk from the user's message.
- Read the relevant plan and status files for that chunk before proposing work.
- Give a concise summary of all implementation steps you will do together.
- Divide the chunk into small steps the user can ask for one at a time, ordered by dependencies.
- Put prerequisite work before dependent work. Prefer this order when it applies: contracts and requirements, configuration, database/schema/storage changes, shared models or types, low-level helpers, service logic, API or command wiring, tests, operational verification, then documentation.
- If a later step depends on a decision or artifact from an earlier step, call that dependency out in the summary.
- Do not edit code during this initial summary unless the user explicitly asks you to begin implementing.
- Ignore documentation updates during the chunk unless the user explicitly says `update the doc`.

## When The User Says `Next step`

Respond with only the next actionable step for the current chunk workflow:

- Follow the dependency-first order from the initial chunk summary.
- Do not skip ahead to a dependent implementation before its prerequisite contract, schema, configuration, helper, or model is in place.
- When the next step needs a low-level helper that is already used by, or is clearly about to be used by, more than one service, put that helper directly in the narrow shared crate before adding service-specific code. Do not first duplicate low-level protocol, signing, endpoint, validation, parsing, escaping, or formatting helpers inside service crates and move them later.
- Keep service-owned behavior out of shared even when shared low-level helpers are used. Queue polling loops, command dispatch, retries, dead-letter decisions, database writes, logging policy, and science-processing workflow stay in the owning service crate.
- Explain the purpose of the step and how it fits the chunk.
- Give exact code, commands, file paths, and edits the user needs.
- Split the step into smaller substeps when a single edit would be too large.
- Keep documentation changes out of the step unless the step is specifically `update the doc`.
- If the next step requires choosing between approaches, briefly explain the tradeoff and recommend one.
- Do not apply the edits or run the commands yourself unless the user explicitly asks you to implement or run them.

## When The User Uses `[Ask]` Or `[Details]`

Treat `[Ask]` and `[Details]` as questions or clarification requests, not as permission to advance the workflow.

- Answer the question directly.
- Reference relevant files or code where useful.
- Do not move to the next implementation step unless the user also asks for it.
- Do not edit files or run commands unless the user explicitly asks for implementation or verification.

## When The User Says `Update the doc`

At the end of the chunk, update all relevant documentation and planning/status files so they reflect the work completed for the chunk.

- Review the work completed in the chunk before editing docs.
- Update only documentation that is actually affected.
- Preserve the repo's existing documentation style and structure.
