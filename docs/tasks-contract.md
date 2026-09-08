# Tasks module public contract

Frozen for [issue #149](https://github.com/azerothl/akasha-os/issues/149) lot 0.
Machine-checked in `crates/aos-proto/src/tasks_contract.rs`.

## Module

- **Name:** `tasks`
- **Package:** `share/modules/tasks.aospkg` (sources: `modules/tasks/`)

## Tools

| Tool id | Purpose |
|---------|---------|
| `tasks.create` | Create task (`title` required, `notes` optional) |
| `tasks.list` | List all tasks |
| `tasks.update` | Update fields by `id` |
| `tasks.complete` | Set `done` (default complete; reopen with `done: false`) |

## Storage

- **Path:** `/documents/tasks/tasks.json`
- **Format:** JSON object `{ "tasks": [ { "id", "title", "notes", "done", "created_ms", "updated_ms" } ] }`
- **Id pattern:** `task-{n}` where `n` comes from internal `stamp()` (not wall-clock ms despite field names)

## Capabilities

| Capability | Use |
|------------|-----|
| `fs.read:/documents/tasks/**` | Read store |
| `fs.write:/documents/tasks/**` | Write store |
| `tool.invoke:tasks` | Invoke any `tasks.*` tool via `module.invoke` |

## Load behaviour (known gap)

`modules/tasks/src/lib.rs` `load()` treats **every** `fs_read` failure as an empty
store (first launch). That preserves missing-file behaviour but can hide permission
errors. **Required follow-up:** distinguish missing file from other read errors before
extraction is complete. Do not change silently without tests.

## Not in this contract

- `task.assess`, `goal.*`, `schedule.*` — platform services, not the Tasks app
- Subtasks, due dates, reminders, calendar integration
