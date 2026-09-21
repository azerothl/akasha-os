# Opt-in transient Ollama profile

Real local Gemma comparison through the native Ollama chat endpoint. The same
source/candidate gardener images were sent using the production backend's
bounded image encoder, not a separate image-generation tool.

The call completed and the first `/api/ps` check reported the selected Gemma
model absent. `residency-after.json` records this observation. No separate
unload/stop command or service termination was used. GPU memory subsequently
measured 3,496 / 16,376 MiB, versus 15,747 MiB during the prior concurrent run;
this is an observation, not a controlled performance comparison.

`ollama-transient` is an explicit provider preset; ordinary `ollama` and other
providers retain their previous transport. The transient profile is loopback
only, uses `/api/chat` with `keep_alive: 0`, `think: false` and a 9,216-token
context. It incurs model reload cost in exchange for releasing resources after
each request. Existing privacy checks and image limits still apply.

The actual response remains a poor critic: it describes smoke in the original,
which has none, and misses the substantive footwear correction. Transport and
memory release success are **not** artistic approval.

Separately, worker and room runtime now poll a known running illustration job
without further model calls or consuming action steps. Worker reflection is
also skipped while such a job is pending. The UI's independent pass publication
continues. Poll errors are retried, not treated as permission to regenerate;
goal/observation deadlines and cancellation remain applicable.

Tests: four backend tests, eight illustration agent tests, and agent binary
checks pass. Live combined generation is still required to validate scheduling.

Protocol reference:
https://github.com/ollama/ollama/blob/main/docs/api.md
