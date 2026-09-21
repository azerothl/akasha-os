# Illustration assets

Logical prefix: `/assets/illustration/**`  
Cap: `asset.read:/assets/illustration/**` (fail-closed; `..` denied)

## Packs

| Path | Contents |
|------|----------|
| `primitives/pack.yaml` | `prop.box`, `humanoid.placeholder` (ADR 0011 metres, Y-up) |

Host loads the primitives pack from the embedded copy in `aos-scene` (offline). On-disk files under `share/assets/illustration/` are the source of truth for that embed.
