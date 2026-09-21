# Desktop verification — 2026-09-19

Real compiled `aos-busd`, `aos-platformd` and `aos-ui-egui`, isolated AOS_HOME
`E:/akasha-os/var/illustration-ui-test01`, bus `127.0.0.1:24719`.
Session `sess-1789842991229`, run `de931118dbc3b1e5-ce76ec900138d49d`.
No personal session store used. Native window observed through Computer Use.

Input: original French cat jump/lie request, with the automatically generated
construction and single-frame brief from `klein4b-auto-cat-sequence-04`.
Initiated through real IPC using the `illustration_ipc_probe` example, not through
an agent conversation. No model daemon was started in this isolated environment.

## Verified

- Full desktop/platform build succeeded; platform/agent/UI all-target check passed
  before addition of the probe. The probe then compiled and executed successfully.
- Five IPC publications: pose 7.9s, volumes 15.6s, contours 22.5s, details 29.6s,
  final 36.8s; final state needs_review, not artistic approval.
- Actual window showed the pose image while generation was running, then the
  contour image while still running, then the final raster with review-required
  text. These were real model outputs, not a simulated reveal of a final image.
- Selecting Pose in pass history after completion showed the original pose PNG.
- Two agent image-reference tests passed (current preview and ordered pending
  source/candidate pair); store test for stale/terminal protection passed.

## Not verified / issues

- Retouch comparison/rejection and export verified below; acceptance of a useful
  candidate remains untested through the desktop.
- No autonomous agent planning/vision loop through the desktop was tested.
- Only pose/contours/final were captured live; volumes/details publication was
  verified by IPC, not separate live screenshots.
- Art still needs proportion/landing corrections; no lying frame or animation.
- Shell shows generic missing-service errors because model/agent services are
  absent. These do not prevent the illustration probe but this is not a complete
  user-session acceptance test.
- Session titles intercept clicks: selecting the fixture required clicking blank
  space in its row. Palette row also clips at the panel's narrow width.

Current test processes were left running for further retouch verification:
bus PID 61184, platform PID 50596, UI PID 61704. Revalidate before operating on them.

## Retouch rejection and export — live desktop follow-up

The same process IDs and single returned native window were revalidated before
interaction. The pending candidate `beae3697192f5055-6cae678d7ebb4a96` visibly
contained an unwanted second cat. Clicking **Conserver l'original** restored the
single-cat source, removed the pending comparison controls and enabled Export.
Persisted `candidate_selected` is false and `last_png` is the original
`de931118dbc3b1e5-ce76ec900138d49d-final.png`.

Clicking Export created
`/downloads/illustration/illust-sess-1789842991229-1789845399121.png`.
Its SHA-256 equals the original source exactly:
`630DB0EBC1F2230F444E90FDEAA1B2918E8D56C4C7B0BA2DE0E781CDFD5234F6`.
The six-image history remains intact. Selecting Final revision 3 shows the
rejected two-cat image without changing the persisted choice or selected PNG.
Returning to **Suivre la génération** restores the original preview.

The desktop build used here predates the pose-reference service integration;
this verifies existing comparison/rejection/history/export behavior, not the new
guide API or a rebuilt complete autonomous desktop flow. The displayed history
does not clearly label a rejected candidate, and the Export button refers to the
selected image rather than whichever historical image is currently previewed.
That distinction should be made clearer in the UI.

## History/export clarification — rebuilt UI

Rebuilt only the isolated desktop UI after the previous observation. UI PID
61704 was revalidated and replaced by PID 55212; bus/platform remained on their
existing isolated instances. The new UI reconnects to bus 24719 and the same
fixture session. Its new strings are localized in English/French (the running
fixture uses English). A unit test passes for pending, rejected, retained and
historical image classification.

Observed in the real window: **Export selected image** fits in the header; the
history lists **Final · 2 · Selected for export** and **Final · 3 · Rejected edit**.
Opening the rejected revision shows its image, an explicit rejection label, and
the notice that exporting saves the selected image rather than this preview.
**Show selected image** returns to the single-cat original and removes the notice.
The text fits the tested panel width without clipping. No broad responsive or
200% zoom validation was performed; existing mixed-language copy elsewhere in
the panel remains outside this targeted clarification.

This closes the history/export ambiguity above for the current candidate/run.
Older runs do not yet retain independent decision metadata, so their rejection
status cannot be reconstructed after a newer run replaces image_run.
