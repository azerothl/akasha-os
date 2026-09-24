# TripoSR experimental adapter

Illustration Studio keeps TRELLIS as the default image-to-3D converter. The
TripoSR option is an opt-in comparison, not an automatic download.

Set `AOS_TRIPOSR_RUNNER` to the absolute path of a local executable before
launching the Preview. Illustration Studio invokes it with exactly two
arguments:

```
<runner> <absolute-input-image-path> <absolute-output-glb-path>
```

The runner must exit successfully and write a **real binary GLB** at the
output path within ten minutes. The host checks the GLB header and loads its
triangles before registering the asset. A failed conversion leaves the project
library unchanged and displays the runner's last error line beside the action.
The runner and model weights remain outside the Akasha package.

The upstream TripoSR `run.py` is a useful starting point, but the textured
`--bake-texture` path emitted an OBJ text file named `mesh.glb` in our local
trial. A runner must import that OBJ and its `texture.png`, attach the texture,
and export a valid GLB (for example with Blender). Do not pass `run.py`
directly as `AOS_TRIPOSR_RUNNER`.

The option is intended for controlled trials. On the same 512×768 full-body
source image, TripoSR produced a much lighter mesh (~33,000 faces versus
~147,000 for TRELLIS at 512) but visibly lower texture and shape fidelity.
Neither model is an automatic rigging solution; UniRig and SkinTokens are
evaluated separately.

Sources: [TripoSR](https://github.com/VAST-AI-Research/TripoSR),
[UniRig](https://github.com/VAST-AI-Research/UniRig),
[SkinTokens](https://github.com/VAST-AI-Research/SkinTokens).
