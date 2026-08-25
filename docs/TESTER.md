# Tester protocol — Akasha OS Preview 0.12.1

**Language:** English | [Français](fr/TESTER.md)

> Date: 25/08/2026 · Preview **0.12.1**

Thank you for testing Preview. Goal: install **without** `cargo` or cloning
the repo, exercise the main paths, and send feedback **from the UI**.
Feature catalogue: [FEATURES.md](FEATURES.md).

## Before you start

- Windows or Linux x64 (NVIDIA recommended; CPU-only package / path also OK)
- Install: see [INSTALL.md](INSTALL.md)
- Launch **Akasha OS Preview** (`aos-session`)

Expected banner: *Preview on Windows/Linux — this is not the bootable OS yet*.

## Steps (also in the Scenarios tab)

### 1. Offline chat

- Finish **model setup** (first run) and the **tutorial** (4 steps).
- **Chat** tab: ask a question (e.g. “What is Akasha OS?”).
- Optionally change the **session model** combo (Models tab lists offerings).
- Verify a streamed reply **without network**.
- After one reply, sidebar / **Models** should show **TTFT** and **tok/s** (and VRAM when on GPU).
- After a **second** turn that reuses the same long context, TTFT should drop
  vs a cold prefill (E20 prefix cache). Optional **draft** / **prefix** metrics
  may appear on the same line when speculative lookup fires (RAG / quote /
  edit-style prompts).

### 1b. Models tab

- Open **Models**: see installed entries, **Download** an alternative if offered.
- Banner “Models: …” when newer offerings fit your VRAM tier.
- Confirm live metrics for the loaded model.

### 1c. CPU-only (optional)

- On a machine without NVIDIA, or with Settings → Inference → **CPU only**:
  Preview should **migrate in-process** (the live reply continues; no cancelled
  turn). `aos-modeld-cpu` is only for hosts without NVIDIA. GPU pin requires NVIDIA.

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
- After first run, `var/secrets/master.backend` is `keyring` or `file`. If
  `keyring`, `master.key` should be absent. Headless Linux may keep a 0600 file.
- **List configured** shows names only (never values).

### 7c. Module cap review (P04.4)

- When an agent (or UI) installs a module without `approved_caps`, a
  **confirmation** lists required caps.
- **Accept** → caps granted; **Refuse** → module quarantined / empty caps.

### 7d. Auto-remember from chat (P05 / E14)

- **Settings → Network**: **Auto-remember from chat** is **on** by default (uncheck to disable).
- In Chat, say something durable e.g. “I prefer French for the UI”.
- After the reply, status should mention fact(s) remembered; **Memory → List**
  shows the fact with a **`[chat]`** badge.
- Say the opposite (“I prefer English”); expect auto-link / `supersedes`.
- Paste a fake key (`sk-abcdefghijklmnopqrstuvwxyz1234` or `ghp_…`) in chat:
  it must **not** appear as a remembered fact (audit may show `filtered`).
- Turn the setting **off** again: further chat must not write new facts.

### 7e. Agent asks the user (`user.ask`)

- Start `/agent` with a task that needs a preference (format, name, choice).
- When the agent asks, the chat hint becomes “answer the agent’s question”;
  type the reply in the same thread (or **Répondre** on the card if several wait).
- The agent should resume with your answer. Leaving it unanswered ~10 min
  continues the task without blocking forever.

### 7f. OS keyring (P06.3)

- After Settings → Secrets **Save**, restart Preview: the key still works.
- `var/secrets/master.backend` is `keyring` or `file`. On Windows, expect
  `keyring` and no readable `master.key`.

### 7g. Local signed catalogue (P06.4)

- **Settings → Local module catalogue**: notes / tasks / ext-rt listed.
- **Install** on a listed module → cap review confirmation (same as 7c).
- Tampering with a packaged WASM while keeping the catalogue entry must refuse.

### 7h. Chat Stop + Copy (P06.5)

- During a streaming reply, **Stop** interrupts generation.
- **Copy** on a chat line (or Troubleshoot / Feedback body) puts text on the clipboard.

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

### 15. Create a module via agent (0.7.0 / E15)

- **Scenarios** tab → **Launch agent: create module cohortmod**
  (or Chat: « crée un module ping » — Preview spawns an agent even if the
  model dumps UI JSON; or Agents / `/agent` with scaffold + package + install).
- Accept the **cap review** for `module.install` if prompted (same as 7c).
- After the agent finishes, a new sidebar tab **Modules → cohortmod** appears
  (not `notes` / `tasks` / `ext-rt`).
- Open it: heading, form or button, table bound to the primary tool.
- Submit once; the result should refresh. Check the scenario box when done.

### 14. Declarative module UI (0.7.0 / E15)

- In **Settings → Modules** (or via agent): `module.scaffold` a script module,
  then `module.package` and `module.install` with cap review.
- After install, a new sidebar tab under **Modules** should appear (not for
  `notes`, `tasks`, or `ext-rt`).
- Open the tab: you should see a heading, a form or button, and a table bound
  to the primary tool result.
- Submit the form or click the button: confirm cap review if prompted; result
  should refresh in the UI.
- **Refresh** reloads bind tools; invalid `ui/index.html` shows an error banner
  (no partial widgets).

### 13. Notes after update + Troubleshoot (0.2.0)

- After installing over a previous Preview, open **Notes**, create a note, then
  read it from the list. The packaged WASM must match this release.
- **Troubleshoot** (Help / sidebar): runs diagnostics (NVIDIA, home, logs).
  If findings exist, a GitHub report can open.

### 16. Uninstall a module (0.8.0 / P08.7)

- Scaffold + install a non-bundled module (step 14 or 15).
- **Settings → Installed modules**: Uninstall (not `notes` / `tasks` / `ext-rt`).
- Confirm the banner. Tab gone, `tool.invoke:<name>` gone from Caps, audit line present.
- Re-install still works.

### 17. E15 widgets (0.8.0 / P08.11)

- Scaffold/install a script module whose UI uses `select` (or an `enum` form field),
  `checkbox` or `radio`, and `bar_chart`. Submit once; result refreshes.
- Invalid `kind` still shows an error banner (whole document refused).

### 18. Providers (0.8.0 / P08.12)

- **Providers** tab: add **Ollama** (or LM Studio) at `127.0.0.1` — Test works
  with **local_only** (no Allow network).
- Add a cloud preset (OpenAI / OpenRouter): key in the vault, never in YAML.
  Chat combo shows the provider model only when routing is **balanced** /
  **remote_only** and Allow network is on.
- `secret` data still never goes remote.

### 19. Image + TTS (0.8.0 / E16)

- Optional: **Models** → **Download** `Stable Diffusion 1.5` and a Piper voice
  (not in the zip; first-run skips them). The same download installs the
  **engine** (`bin/sd.exe` / `bin/piper.exe` + DLLs) if it is missing.
- Restart Preview so `etc/modeld.yaml` lists the pack.
- Chat: `/image a red cube` then `/speak hello` (or French with `piper-fr-fr`).
- PNG appears in the conversation; **Open in studio** switches to the Image tab with the prompt filled.
- `/speak` opens an **in-chat TTS card** (voice + knobs) — Generate writes the WAV. **Play** on the clip. Files under `/downloads`.
- Without the pack **or** if the engine zip fails, a stub PNG (color bars) /
  short WAV is still written so the cap / `/downloads` path is testable.

### 21. Mid-token migrate (0.9.0 / E18)

- Start a **long** chat completion.
- While tokens stream, Settings → Inference: switch **gpu ↔ cpu** (NVIDIA host).
- The reply must continue on the **same** turn — no Stop, no « cancelled » line.
- Audit may show `model.migrate`. If migrate fails, fallback 0.8 restart is audited as `model.migrate.fallback`.

### 22. Extra media packs + closed options (0.9.0 / E19)

- Models: list `local:flux2` / `local:ideogram4` / `local:piper-en-gb` (optional Download).
- Settings: pick a non-default image pack + width/steps; pick a Piper voice. Restart.
- `/image` must use that pack/size. An unknown option key on the intent is refused (audit `media.options.refuse`).
- Image studio tab: set sampler / negative / catalogue LoRA-VAE if extras exist → Generate.

### 24. Image studio depth (0.10.0 / Media polish)

- Image studio: add overlapping **composition** blocks on the aspect canvas; Generate injects layout/JSON into the prompt.
- Optional: Download a RealESRGAN / upscale pack → enable **Upscale** on a preview PNG (`media.image.upscale`).
- Expert mode (DiT / advanced knobs) is optional; Wan/LTX rows are **experimental** — not required for cohort pass.
- Models Image/Audio tabs show the optional media-packs blurb.

### 25. Vault TPM + sibling bridge (0.10.0–0.10.1 / E7–E8)

- Settings / secrets: after restart, `var/secrets/master.backend` may read `tpm` only when the master was sealed with Platform Crypto (`TPM2` blob); TPM presence alone is not enough. Otherwise `keyring` or `file`.
- Optional: start `aos-bridged` from Preview `bin/` (loopback only) against a running session; health + `mem.context` + `mem.stats` / `mem.list` succeed; agent-style `X-Aos-From` on `secrets.get` returns 403. Smoke: `.\demo\smoke-bridge.ps1`.

### 26. Updates auto-download + img2img (0.10.1)

- Settings: enable **Auto-download updates** (off by default). After a newer Release is detected, `var/updates/pending.json` appears and the banner says relaunch to apply (no Download click needed).
- Image studio: enable **Start from an image**, pick `/downloads/…` or **Use current preview**, set strength → Generate (img2img). Inpaint/mask not required.

### 27. Local decode (0.11.0 / E20)

- After a **second** chat turn that reuses the same long context, TTFT on Models / sidebar should drop vs the first (cold) turn.
- Optional **draft** / **prefix** metrics may appear on that line when prompt-lookup fires (quotes / RAG / repeated prefixes).
- Streamed tokens must still match a non-speculative reply (exact sampler). Multi-agent / batch N>1 still streams.

### 23. One-liner install (0.8.0 / P08.9)

- Windows: `irm https://azerothl.github.io/akasha-os/install.ps1 | iex`
- Linux: `curl -fsSL https://azerothl.github.io/akasha-os/install.sh | sh`
- Script must print URL + sha256, refuse on mismatch, overlay into the stable
  prefix without wiping `var/`.

## Success criteria (team)

- 3 Windows + 1 Linux testers complete this protocol without a Rust toolchain
- At least one usable `var/feedback/fb-*.json` per report
- Gates PC.6–PC.9 and PC.11–PC.13 checked on at least one machine

## Out of scope Preview 0.11.0

- seL4 / bare-metal boot
- macOS
- 32B model in the installer
- Product video UI / STT / always-on voice
- Inpaint / mask as a first-class intent
- Public marketplace / messaging channels / multi-GPU hard-green
- `webview` widget kind
