# Vision: an agent-native OS designed from scratch

**Language:** English | [Français](fr/reflexion-agent-os.md)

> Date: 11/08/2026
> Context: reflection on the feasibility of an operating system built from the outset around agents (AI), without a Windows/Linux base, offering agents great freedom while remaining performant, pleasant for the user, and extensible through modules/applications accessible to both agents and humans.

---

## Initial framing

Before listing possible directions, a fundamental question must be settled: **what really needs to be reinvented, and what should not be?** Starting from scratch with drivers, low-level scheduling, the TCP/IP stack, etc. is a project of several hundred engineer-years (see seL4, Fuchsia/Zircon, Redox). The real “agent-first” innovation is not at the low level, but in **the layer between the kernel and applications**: how the system models agents’ intent, memory, trust, and capabilities. That is where the effort should be concentrated.

### Existing research to know about (do not reinvent the wheel)

- **AIOS (LLM Agent Operating System)** — Rutgers/agiresearch, COLM 2025 paper (arXiv:2403.16971): a kernel that manages scheduling, context switching, memory, storage, and tools for LLM agents, with up to 2.1x acceleration vs. naive execution. Repo: github.com/agiresearch/AIOS
- **LSFS** (LLM-based Semantic File System, ICLR 2025): a file system driven by prompts rather than commands. arXiv:2410.11843
- **A-MEM**: long-term agentic memory. arXiv:2502.12110
- **Cerebrum**: an agent SDK decoupled from the kernel (repo agiresearch/Cerebrum).
- **LiteCUA**: a “computer-use” architecture with VM Controller + MCP server in a sandbox. arXiv:2505.18829
- Relevant classic OS references: **seL4** (formally verified, capability-based microkernel), **Fuchsia/Zircon** (Google, capability-based, component framework), **Genode OS Framework** (conceptually the closest: isolated components, explicit capabilities, extensible from micro-embedded systems to desktop), **Redox OS** (microkernel in Rust).

These projects remain above Linux—which is revealing: even when aiming for an “agent OS,” nobody reinvents the low level. Recommendation: take the same stance, with the ambition of going further on agent-specific layers.

A dated comparison of agentic OS / runtime projects (including the related Akasha assistant and OpenClaw-class stacks) lives in [competitive-landscape.md](competitive-landscape.md).

---

## 1. Kernel architecture: capabilities, not ambient permissions

The most important principle for **safe** “total freedom” for agents is not the absence of control, but a **capability-based security** model, as in seL4, Fuchsia/Zircon, or Genode.

- An agent can “do anything” only within the capabilities delegated to it (file X, network Y, execution of a given tool)—each capability is an unforgeable, revocable token with a lifetime.
- No global Unix-style permissions (uid/gid): too coarse for an autonomous agent that acts quickly and at scale.
- Paradoxically, this provides *more* operational freedom, because broad capabilities can be granted to a trusted agent without risking total system compromise—and everything is auditable/revocable after the fact.
- Microkernel rather than monolithic: strong isolation between the kernel, drivers, and the “agent runtime,” so that agents’ memory/security cannot crash the system.

## 2. “Semantic syscalls” rather than POSIX syscalls

The agent should not communicate with the kernel through `open()/read()/write()`, but through **structured intents** that the kernel translates into low-level primitives.

- A semantic system-call layer: `intent("summarize this document and notify the user if urgent")` breaks down into a chain of calls (file reading, LLM inference, notification) orchestrated by the kernel, not by the agent itself.
- Standardize this protocol as **MCP (Model Context Protocol)** does today for tools—but make it the system’s native IPC protocol, not an application layer added after the fact. Each app/module exposes its capabilities through a dynamically discoverable typed schema (introspection), consumed equally by agents and human UI.
- The semantic filesystem (LSFS) is a good direction: data is addressed by description/intent, with a classic POSIX layer underneath for performance and compatibility (do not throw away the file hierarchy; enrich it).

## 3. Scheduler aware of the nature of agent workloads

The real performance bottleneck is almost never the CPU/kernel—it is **LLM inference latency** (I/O-bound, not CPU-bound) and GPU/NPU access. The scheduler must be designed around this from the outset:

- Asynchronous/cooperative scheduling (green threads) where an agent waiting for an LLM token immediately frees the logical core—not blocking in the manner of a classic OS thread.
- **Native batching of inference requests** at the kernel level: if 20 agents call the same model within 50ms, the kernel should be able to batch on the GPU rather than let each agent make an isolated request (a huge gain observed by vLLM, TGI, etc.—to be done at the OS level rather than in each application).
- The GPU/NPU must be a **first-class citizen of the scheduler**, not a secondary device controlled by an application driver—this is the opposite of Linux historically.
- The notion of a “cognitive context switch”: saving/restoring an agent’s complete state (context, working memory, active capabilities) as one saves CPU registers—necessary for fair preemption among competing agents.
- Prioritization by trust/criticality rather than simple round-robin (an agent handling a payment does not have the same priority as a monitoring agent).

## 4. Multi-level memory (not just RAM/swap)

An agent-native OS needs a specific memory hierarchy:
- **Working memory** (short-term context, the LLM’s context window)—managed like a fast cache.
- **Episodic/long-term memory**—a native vector database (embeddings), with a *semantic* eviction policy (relevance) rather than classic LRU. A-MEM is a good starting reference.
- **Inter-agent shared memory** with capability-based access control—for multi-agent collaboration without unwanted context leakage.

## 5. Universal module/app sandboxing: WASM/WASI

For extensibility (modules/apps consumable by BOTH agents AND humans):
- Universal package format based on **WASM/WASI**: portable, sandboxed by construction, executable server-side or at the edge, with a capability model that exactly matches the kernel model (no implicit syscall access).
- Each module exposes a **dual manifest**: (a) a tool/function schema consumable by agents (function calling / MCP-like), (b) a rendering interface for humans (declarative UI). Same artifact, two consumption surfaces.
- Marketplace/registry with signing and attestation of requested capabilities (like a mobile store, but with much more granular capabilities that can be formally verified if possible).
- Versioning and compatibility: agents must be able to dynamically discover a module’s capabilities (schema introspection) without redeploying the system.

## 6. Security and trust: the real critical subject

“Total freedom” is the most dangerous point of the project. Concrete recommendations:
- **Reversibility by default**: every agent action with a side effect (file, network, purchase) must be designed to be undoable (transactions, ZFS/btrfs-style snapshots, undo log)—the real obstacle to trust is not capability, but irreversibility.
- **Complete and readable audit trail**: every agent decision (prompt, capability used, result) traced natively at the kernel level, not as an optional application feature.
- **Graduated autonomy**: an agent gains broader capabilities with a trust history (like a behavioral “credit score” system), rather than total access from installation.
- Strict isolation between unrelated agents (no implicit shared memory) to prevent prompt/context contamination between different tasks.

## 7. Human user experience

The classic trap of an OS “for agents” is forgetting the human. Possible directions:
- **Dual interaction surface**: direct mode (classic GUI, click/keyboard) and conversational/delegated mode, coexisting on the same data/capabilities—the user chooses their level of involvement at any time.
- **Reasoning transparency**: the UI must be able to show *why* an agent did what it did (chain of custody of decisions), without drowning the user—configurable levels of detail.
- **Real-time interruption/steering**: the ability to take back control at any time over a delegated task without losing the context already built.
- Intelligent notifications prioritized by a “supervisor agent” rather than the current cacophony of classic OSes.

## 8. Realistic implementation approach (roadmap)

Given the cost of reinventing a true low level:

1. **Phase 1 — Prototype of the agent layer** on an existing kernel (Linux, or a microkernel such as seL4/Redox for security): implement the capability model, the agent-aware scheduler, the semantic IPC protocol, and WASM sandboxing. This is essentially what AIOS does, but we can go further on capabilities and sandboxing.
2. **Phase 2 — Native system components**: semantic filesystem, native vector memory, GPU/NPU management as a first-class citizen of the scheduler.
3. **Phase 3 — A true dedicated kernel** only if the preceding phases demonstrate structural limitations of the host kernel (IPC latency, insufficient isolation)—probably based on a capability-based microkernel in Rust (inspired by seL4/Genode/Redox) rather than starting completely from scratch.

## Points of caution

- Do not confuse “agent freedom” with “absence of control”—the capability model provides both at once (broad freedom of action + traceability/revocability).
- The real performance risk is not the kernel but inference—design the scheduler around this from day one, not as an afterthought.
- Reinventing hardware drivers (Wi-Fi, GPU, USB...) from scratch is a bottomless pit; relying on existing hardware abstraction layers (via a lightweight hypervisor or a microkernel that exposes user-space drivers) is realistic, while reinventing ACPI/PCIe is not.

---

## Directions to explore later — progress status

> Update (v0.2 of the specs) following the completeness review of `docs/functional-specs.md` and `docs/technical-specs.md`.

| Initial direction | Status | Reference |
|---|---|---|
| Detailed capability model (format, revocation, delegation) | Addressed | `docs/technical-specs.md` §2.3 |
| Design of the agent-aware scheduler (batching, preemption) | Addressed | `docs/technical-specs.md` §3.6, §3.6.1, §3.5.5 |
| Semantic filesystem architecture | Addressed (v1) | `docs/technical-specs.md` §6 |
| Dual manifest format (agents + UI) | Addressed | `docs/technical-specs.md` §7 |
| Economic model / module marketplace | **Still open** | local registry only in v1; distribution/monetization out of scope (`docs/functional-specs.md` §2.2) |

### New directions identified during the completeness review

- Multi-user support (separate accounts)—currently out of scope for v1, single user + multiple agents (`docs/functional-specs.md` §2.3).
- Mechanism for updating the OS itself (A/B image, rollback)—now outlined (`docs/technical-specs.md` §10.1), to be explored further during implementation.
- Trust score / graduated autonomy—now detailed (`docs/technical-specs.md` §4.7, Trust Manager), remains to be empirically validated (which factors weigh the most).
- Energy/thermal management for the Placement Manager on mobile hardware (aarch64 laptop/edge)—not addressed, not a priority as long as the target remains desktop/server.
- Egress network control for generic modules (beyond model backends)—now addressed (`docs/technical-specs.md` §9.5).
