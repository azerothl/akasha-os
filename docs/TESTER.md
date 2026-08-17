# Tester protocol — Akasha OS Preview

**Language:** English | [Français](fr/TESTER.md)

Thank you for testing Preview. Goal: install **without** `cargo` or cloning
the repo, exercise the main paths, and send feedback **from the UI**.
Feature catalogue: [FEATURES.md](FEATURES.md).

## Before you start

- Windows or Linux x64 (NVIDIA recommended; CPU-only package / path also OK)
- Install: see [INSTALL.md](../INSTALL.md)
- Launch **Akasha OS Preview** (`aos-session`)

Expected banner: *Preview on Windows/Linux — this is not the bootable OS yet*.

## Steps (also in the Scenarios tab)

### 1. Offline chat

- Finish **model setup** (first run) and the **tutorial** (4 steps).
- **Chat** tab: ask a question (e.g. “What is Akasha OS?”).
- Optionally change the **session model** combo (Models tab lists offerings).
- Verify a streamed reply **without network**.
- After one reply, sidebar / **Models** should show **TTFT** and **tok/s** (and VRAM when on GPU).

### 1b. Models tab

- Open **Models**: see installed entries, **Download** an alternative if offered.
- Banner “Models: …” when newer offerings fit your VRAM tier.
- Confirm live metrics for the loaded model.

### 1c. CPU-only (optional)

- On a machine without NVIDIA, or with Settings → Inference → **CPU only** then restart:
  Preview should start and local chat should work (slow is OK).

### 2. Human note

- **Notes** tab → title + body → **Create**, then **List**.

### 2b. Tasks (dual-surface)

- **Tasks** tab → create a task.
- Start an agent with tools including `tasks.list` — it should see the same task.
- Optionally ask the agent to `tasks.create`; refresh the Tasks tab.

### 3. Note via agent

- **Agents** tab → create an agent with a task like
  “create a note titled cohort with content hello”.
- The agent uses the `TOOL:` convention on the model side.

### 4. Sensitive confirmation

- When a confirmation banner appears (sensitive action):
  **Deny** once, then **Accept** another (or the same replayed).
- Fail-closed: timeout = deny.

### 5. Audit + caps + kill auditd

- **Audit** tab → **Refresh** (signed events).
- **Caps** tab: load holder `agent:<id>` for an active agent → see caps → **Revoke** a non-critical one → confirm an audit line.
- **Kill aos-auditd**: chat must continue; the supervisor restarts auditd
  in the background.

### 5b. Scheduler

- **Settings** → Schedules: create a schedule with interval **60s** and a short goal.
- Wait for a fire: a new agent should appear; cancel the schedule so it does not fire again.

### 6. Parallel sessions (PC.6)

- **Sessions** panel (Chat): create 3 sessions, chat in each.
- Restart Preview: histories must reappear.

### 7. Memory (PC.7 / P04)

- **Memory** tab: remember “I prefer French”, **List**, then remember “I prefer English”.
- Expect auto-link (`supersedes` / `updates`); **Recall** should prefer English.
- Edit / delete / supersede from the list; toggle “Show superseded”.
- Back to Chat: the next message should use that context (`mem.context`).

### 7b. Secrets vault (P04.3)

- **Settings → Secrets**: set a Brave (or GitHub) key → **Save**.
- Confirm status mentions encrypted vault; `var/secrets/keys.yaml` should be
  absent or renamed `.migrated` (live store is `vault.enc`).
- **List configured** shows names only (never values).

### 7c. Module cap review (P04.4)

- When an agent (or UI) installs a module without `approved_caps`, a
  **confirmation** lists required caps.
- **Accept** → caps granted; **Refuse** → module quarantined / empty caps.

### 7d. Auto-remember from chat (P05 / E14)

- **Settings → Network** (or Memory-related prefs): enable **Auto-remember from chat**
  (default is **off**).
- In Chat, say something durable e.g. “I prefer French for the UI”.
- After the reply, status should mention fact(s) remembered; **Memory → List**
  shows the fact with a **`[chat]`** badge.
- Say the opposite (“I prefer English”); expect auto-link / `supersedes`.
- Paste a fake key (`sk-abcdefghijklmnopqrstuvwxyz1234` or `ghp_…`) in chat:
  it must **not** appear as a remembered fact (audit may show `filtered`).
- Turn the setting **off** again: further chat must not write new facts.

### 8. Web search (PC.8 / PC.13)

- **Allow network** (sidebar) **off** → **Search** must fail (`offline_strict`).
- Enable network → search (e.g. “Akasha OS seL4”) → title/URL results.
- Settings → search engine: try `auto`, then force `duckduckgo` or `bing`.
- (Optional) Brave key via **Settings → Secrets** (encrypted vault), not a plaintext file.
  Legacy `var/secrets/keys.yaml` is migrated on boot.

### 8b. Browse a page (PC.13)

- With network ON: paste a URL → **Browse** (`web.browse`).
- Expect title + extracted text (no JavaScript). Compare with **Download URL**
  (`net.fetch`), which saves the raw file under `/downloads`.

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

**No automatic network upload** (except explicit actions: PC.8–9, browse, and
GitHub feedback submit).

### 11. Settings (PC.12)

- **Settings**: switch language en ↔ fr; change default agent model / max steps.
- Restart Preview: preferences in `var/run/preferences.json` must persist.

### 12. Agent transparency (PC.11)

- Start an agent (Agents tab or `/agent` in Chat) with a short task.
- Open **Detail**: timeline of steps, sources if the agent searched/browsed,
  Pause then Resume (or Steer a new directive).
- Complex tasks should show a **complex** badge (`task.assess`) and may spawn
  a child agent (planner).

### 13. Notes after update + Troubleshoot (0.2.0)

- After installing over a previous Preview, open **Notes**, create a note, then
  read it from the list. The packaged WASM must match this release.
- **Troubleshoot** (Help / sidebar): runs diagnostics (NVIDIA, home, logs).
  If findings exist, a GitHub report can open.

## Success criteria (team)

- 3 Windows + 1 Linux testers complete this protocol without a Rust toolchain
- At least one usable `var/feedback/fb-*.json` per report
- Gates PC.6–PC.9 and PC.11–PC.13 checked on at least one machine

## Out of scope Preview 0.5.0

- seL4 / bare-metal boot
- macOS
- 32B model in the installer
- Fully automatic update apply
- Native audio/video generation
- Public marketplace / messaging channels / multi-GPU
