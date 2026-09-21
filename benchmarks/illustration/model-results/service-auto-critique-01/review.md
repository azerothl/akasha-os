# Automatic visual critique and correction: FAILED quality gate

The production image service generated five passes from the original French
gardener brief and the prior automatically generated construction plan. Local
Qwen3.5:9b then viewed each current PNG (not just its description). It authored
two corrections automatically; no correction text was supplied by the operator.
Raw critic responses, judgments and exact input PNGs are saved in this directory.

Seven previews were published. The final correction appeared at 70.4 seconds.
Service state remained needs_review and the exported review remained accepted=false.
Successful exit verifies only publication/export mechanics.

The critic repeatedly described bad leg anatomy but did not identify the visible
construction markers or specify clean trouser fabric. It also asserted floating
chair feet without convincing visual support. Its second prompt asked for smoke
despite an existing large smoke contour.

Manual inspection of final.png: correction worsened the image. Trousers became
skin-colored over parts of the legs, construction circles remain, and an extra
smoke plume was added rather than replacing the original cloud. The third critic
still requests correction. The run stops at its two-edit limit, NOT acceptance.

Conclusion: image-conditioned correction is functional, but this automatic critic
is not reliable enough to choose or approve finished artwork. The successful
manually specified edit in service-refinement-01 must not be described as an
autonomous success. Future critics must describe concrete visible marks and the
desired replacement, and compare candidate vs source for regressions. Retain
both images rather than treating newest as best.

Desktop investigation: Computer Use initialized successfully and listed apps.
No Akasha window was open. The installed old preview was not launched. Desktop
progressive display and real-agent vision turns remain unverified.
