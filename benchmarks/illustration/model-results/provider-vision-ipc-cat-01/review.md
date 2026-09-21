# Live model.infer image pair

The isolated model daemon registered the configured local Ollama Gemma model
with `has_vision=true`. Two existing illustration PNGs, supplied as logical
`/downloads/illustration/...` paths, passed through model.infer and storage
classification. Both indexed files were classified private (not secret).

The actual response correctly reports one cat in the source and a second cat
added in the rejected retouch. Its finer anatomical claims are not validated.
This verifies registration and image transport through the live bus, not the
full drawing agent or artistic quality.

The negative companion run `provider-vision-ipc-unregistered-01` verifies an
unknown logical path is refused with a streamed classification error. The FS
service previously substituted a default class for unknown paths; it now
returns NotFound, allowing modeld to fail closed on image transmission.

Running fixture services after this test: bus 61184, modeld 41968, platform 61760.
Only the isolated platform instance was restarted; user data was not removed.
