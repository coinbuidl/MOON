# Moon Memory Skill

Use this skill to retrieve high-signal information from the long-term session library (`mlib/`).

## Core Tool: `moon-recall`

Before starting a task that requires context from past sessions, search the library:
`moon moon-recall --name history --query "<keywords>"`

### Guidelines

1. **Search First**: If the user refers to prior decisions, technical fixes, or specific conversations not in your current context, run a recall query immediately.
2. **Keyword Optimization**: Use 3-5 specific keywords related to the topic (e.g., "watcher binary path fix" or "venus system margins").
3. **Execution**: Always run via the `moon` binary. Do NOT attempt to rebuild, restart, or modify the MOON system.
4. **Output Usage**: Use the retrieved Markdown projections to inform your reasoning and responses. Do not hallucinate past facts if the recall returns no matches.

## Constraints

- **Read-Only**: You are authorized for **retrieval only**.
- **Prohibited**: Do NOT use `moon-stop`, `moon-stop`, `install`, `repair`, or any `cargo` commands.
- **Scope**: Focus on the `history` collection to find session projections.
