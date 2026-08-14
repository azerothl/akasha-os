# Tester protocol — Agent OS Preview 0.1

**Language:** English | [Français](fr/TESTER.md)

Thank you for testing Preview. Goal: install **without** `cargo` or cloning
the repo, exercise the main paths, and send feedback **from the UI**.

## Before you start

- Windows or Linux x64 + NVIDIA GPU (`nvidia-smi` OK)
- Install: see [INSTALL.md](../INSTALL.md)
- Launch **Agent OS Preview** (`aos-session`)

Expected banner: *Preview on Windows/Linux — this is not the bootable OS yet*.

## Steps (also in the Scenarios tab)

### 1. Offline chat

- Finish the **tutorial** (4 steps: welcome, preferences, tour, paths).
- **Chat** tab: ask a question (e.g. “What is Agent OS?”).
- Verify a streamed reply **without network**.
- On the very first run, GGUF models must have been downloaded (network).

### 2. Human note

- **Notes** tab → title + body → **Create**, then **List**.

### 3. Note via agent

- **Agents** tab → create an agent with a task like
  “create a note titled cohort with content hello”.
- The agent uses the `TOOL:` convention on the model side.

### 4. Sensitive confirmation

- When a confirmation banner appears (sensitive action):
  **Deny** once, then **Accept** another (or the same replayed).
- Fail-closed: timeout = deny.

### 5. Audit + kill auditd

- **Audit** tab → **Refresh** (signed events).
- **Kill aos-auditd**: chat must continue; the supervisor restarts auditd
  in the background.

### 6. Parallel sessions (PC.6)

- **Sessions** panel (Chat): create 3 sessions, chat in each.
- Restart Preview: histories must reappear.

### 7. Memory (PC.7)

- **Memory** tab: remember a fact (“I prefer French”), **Recall**.
- Back to Chat: the next message should be able to use that context
  (`mem.context` injection before infer).

### 8. Web search (PC.8)

- **Allow network** (sidebar) **off** → **Search** must fail (`offline_strict`).
- Enable network → search (e.g. “Agent OS seL4”) → title/URL results.
- (Optional) Brave key in `var/secrets/keys.yaml`:
  `brave_search_api_key: "…"` — otherwise DuckDuckGo HTML.

### 9. Download + file generation (PC.9)

- With network ON: paste an image URL → **Download URL** → file under
  `/downloads` (`var/storage/data/downloads/`).
- **Generate file**: format `pdf` or `png`, path `/downloads/test.pdf`,
  text content → **Open downloads**.

### 10. Feedback from the UI

- **Feedback** tab (or **Report** button):
  - title, category (bug / ux / perf / security), severity, body
  - **Create a GitHub issue** (checked by default, except security)
  - **Send feedback**
- A local copy is written under `var/feedback/`.
- An issue (or prefilled GitHub form) opens on
  [azerothl/akasha-os](https://github.com/azerothl/akasha-os/issues).
  With a GitHub account, confirm **Submit new issue**.

**Security** reports are **not** published.

**No automatic network upload** (except explicit actions: PC.8–9 and
GitHub feedback submit).

## Success criteria (team)

- 3 Windows + 1 Linux testers complete this protocol without a Rust toolchain
- At least one usable `var/feedback/fb-*.json` per report
- Gates PC.6–PC.9 checked on at least one machine

## Out of scope Preview 0.1

- seL4 / bare-metal boot
- macOS, CPU-only
- 32B model in the installer
- Fully automatic update apply
- Native audio/video generation
