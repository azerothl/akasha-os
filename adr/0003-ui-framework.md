# ADR 0003: UI Framework Decision

**Language:** English | [Français](../docs/fr/adr/0003-ui-framework.md)

## Context

Phase P3 validated several user interfaces (TUI, web, etc.). This ADR
formalizes the primary UI framework choice for Agent OS.

## Options considered

| Framework | Pros | Cons |
|-----------|------|------|
| **egui** | Light, fast, good for simple 2D UIs, easy Rust integration | No native complex animations; limited advanced theming |
| **iced** | Performant, modern, native animation support, active community | Harder Rust integration; fewer ready-made templates |
| **tauri** | Web app (HTML/CSS/JS) compiled with Rust; multi-platform | Compile overhead; webview runtime dependency |

## Decision

**Choice: egui**

- **Reasons**:
  - Light and fast to integrate in the development cycle
  - Native Rust support (stable bindings)
  - 2D UI sufficient for conversational assistant and dashboards
  - Active community and abundant docs
  - Easy rapid UX prototyping

- **Fallback**: if needs evolve toward complex web UIs, tauri may be added in
  P5. iced is kept as a contingency (`crates/aos-ui-iced`).

## Impact

- **Phase P1**: egui integration in Model Subsystem v1
- **Phase P2**: custom widgets (charts, tables)
- **Phase P3**: full dashboard with resource panels
- **Phase P4**: UI portable to target machines (ARM64, x86_64)

## Future consequences

- **Maintenance**: egui remains the base, with custom extensions as needed
- **Portability**: UI code encapsulated in a separate module for multi-platform
  deploy
- **Evolution**: web-native via tauri possible in P5 if needed

## References

- [P1.6 Minimal UI](../docs/development-plan.md)
- [Functional specs — Interfaces](../docs/functional-specs.md)
