# Illustration style packs (NPR)

Style packs are **data** under `/assets/illustration/styles/**`.
They describe Non-Photorealistic Rendering appearance (Sketch / Pencil / Ink / Comic-Manga)
separately from the SceneGraph (ADR 0011) and from Render presets (resolution /
samples).

Host crates (`aos-scene`) load YAML styles and apply CPU approximations.
Optional Blender beauty reads the same `style` block from the scene export;
GPL Freestyle / NPR scripts live only in `share/illustration-renderer-pack/`.

See [docs/illustration-npr-styles.md](../../../docs/illustration-npr-styles.md).
