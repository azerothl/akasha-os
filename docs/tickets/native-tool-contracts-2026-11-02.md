# Native tool contracts — #247 / #437 / #438

The conversation checks exposed two integration defects. The native C chat
matcher recognizes Qwen's template but has no parameter for tool declarations,
so the model never receives them. Rendering the complete Jinja template fixes
that omission and produces Qwen's `function` / `parameter` protocol, which the
agent's existing JSON parser could not decode.

Tool-bearing requests now use the Jinja renderer. Later system messages added
by the agent runtime are combined into the first system message in their
original order; non-system turns retain their order and content. The real
worker initially failed with a chat-template error for roles
`system, user, system`; that same project workflow succeeds after normalization.

`parse_actions_with_tools` handles complete native calls against the offered
catalogue. Worker and room execution both use it. It preserves string values,
decodes declared integers, numbers, booleans, nulls, arrays and objects, and
checks required keys, duplicate/unknown parameters, enum/const values and
nested shapes. Native booleans accept `true`/`false` case-insensitively, while
string `null` remains a string. One protocol framing newline is removed from
each end of a parameter; internal whitespace is preserved.

The format/type reference is Qwen's
[pinned parser source](https://huggingface.co/Qwen/Qwen3-Coder-30B-A3B-Instruct/blob/573fa3901e5799703b1e60825b0ec024a4c0f1d3/qwen3coder_tool_parser.py).
Akasha additionally requires complete closed calls, offered tool names and
unambiguous parameters. It never evaluates parameter text. Unsupported schema
references/combinators and malformed or unsupported native dialects are
rejected. Bounds are 128 KiB, 64 calls, 128 parameters per call and 16 nested
schema levels. Tool-host constraints and the existing capability/HITL gate
continue to apply. No capability is derived from model arguments.

Native markers inside complete legacy JSON strings remain argument data.
Malformed native calls cannot be repaired into JSON actions or recover actions
embedded inside parameter text. Existing JSON/DSML execution remains available.
The Spark `arg_key` / `arg_value` dialect remains unsupported and cannot cause
embedded JSON to execute; a Spark-specific parser is outside this change.

## Observed validation

- 419 runtime-agent tests, 25 llama tests with CPU features, 25 with CUDA/mtmd,
  and two conversation-grader tests passed. Strict CPU Clippy passed for the
  agent/llama targets and the conversation example.
- A real Qwen3.5-4B worker completed `workspace.inspect` → `workspace.read` →
  `goal.complete`, all in native function/parameter format. It read the exact
  Unicode path `première.txt` and returned the generated proof code, which was
  not present in its prompt.
- The required permission gate executed both workspace tools, with zero
  legacy ungated steps. Actor audit and source reads were verified. All three
  project files stayed unchanged and the synthetic secret was withheld.
- The test used an isolated home and stopped its owned model daemon. It used
  the existing local Anaconda interpreter and the pinned local akasha-model
  source for the permission bridge, without installing or updating packages.

Raw native calls, canonical arguments, before/after template results, source and
binary hashes are in `evidence/2026-11-02-native-worker-contracts.json`.

Reproduce the real worker with the existing example, absolute executable/model
paths, and `--require-gate`. Configure the Python bridge as described in
`docs/tool-gate-host.md` / `docs/dev-assistant-real-worker.md` and retain
stdout JSON and daemon/event logs.

These are bounded synthetic workflows. Representative code-assistant quality,
installed UI and Metal inference acceptance remain open. #439, #444, #247,
#460, #434 and #435 remain open; Spark stays optional and unranked.
