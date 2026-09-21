# Alternate local planner: Gemma 3 12B

`klein4b-semantic-gemma-cat-01` uses the same current inventory/temporal schema,
original French cat request, renderer and image seed 42 as the Qwen comparison
`klein4b-semantic-auto-cat-02`. Only planner model changes to installed local
`gemma3:12b`. No cloud call or new model download occurred.

The plan correctly keeps the later lying action out of the selected frame. It
is shorter and gives fewer contradictory spatial statements, though front-paw
contact versus approaching contact remains imprecise and the dimensions are not
a dependable scale constraint.

Actual first and final images were inspected. One cat, one tail and recognizable
furniture are present. However the hind paws contact the backrest while the front
paws point away from the seat, making this read as jumping OFF the back rather
than onto the seat. The cat is oversized. The sofa is more cleanly finished than
the Qwen run, but the requested action remains wrong. Rejected. No planner
replacement is justified by this single test.

## Next discriminating experiment

Text/seed/guide ablations have not established reliable pose adherence. Test an
alternative image checkpoint while holding the saved plan and seed fixed.
The upstream sd.cpp guide documents Klein **base 4B** with the same Qwen3 4B/VAE
family, CFG 4 and 20 steps, versus distilled 4B at CFG 1 and 4 steps:
https://github.com/leejet/stable-diffusion.cpp/blob/master/docs/flux2.md

The publisher describes base 4B as undistilled, supporting reference editing,
under Apache 2.0; its Diffusers example uses 50 steps and CFG 4:
https://huggingface.co/black-forest-labs/FLUX.2-klein-base-4B

These are upstream compatibility/settings claims, NOT measured quality gains in
Akasha. Base weights are not yet downloaded or evaluated here. Merely increasing
steps on the existing distilled checkpoint is not the same comparison. Keep the
current production default unchanged until an actual matched comparison exists.
