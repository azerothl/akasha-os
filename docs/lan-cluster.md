# LAN cluster configuration

The LAN cluster is experimental and disabled by default. It only uses nodes
that you add and explicitly pair; it never discovers or contacts Internet
peers.

## Configure it in the UI

1. Open **Settings → Models** and enable **LAN cluster**.
2. Set the local node identity, the advertised listener address, and the name
   of the session-key secret. The default secret name is
   `lan_cluster_session_key`.
3. Set the local public-key fingerprint, enable **Auto-discover LAN
   candidates**, and keep the same UDP discovery port on the participating
   hosts.
4. Enable **Allow model transfer to LAN** only after accepting that a GGUF
   file may be copied to a paired worker. This consent is independent from
   enabling the cluster and is disabled by default.
5. Open **Settings → Secrets vault**, enter a random 32-byte key as exactly 64
   hexadecimal characters, and save it under that secret name. The value is
   write-only in the UI.
6. In the LAN cluster section, enter the remote node ID, display name, LAN
   address (`192.168.x.y:port`, loopback and link-local addresses are also
   accepted), and its public-key fingerprint.
7. Select **Add node**, or wait for a discovery candidate, then select
   **Pair** after checking the fingerprint.
   **Revoke** immediately excludes a node from future work.

The local identity, announced address and secret name are stored in the local
preferences file. The node inventory and trust state are persisted in
`var/run/lan-pairing.json`.

## Key and pairing rules

- Use a different random session key for each trusted LAN group.
- Never paste a prompt, model, or private key into the node fingerprint field.
- A changed fingerprint for an existing node is rejected.
- A node is not eligible for work until it is paired.
- Revocation is fail-closed and survives a restart.
- Discovery advertisements are unauthenticated hints. The source IP must
  match the advertised LAN address; a changed fingerprint is rejected and a
  new candidate always remains `Unpaired`.

## Current execution state

`model.cluster.plan` computes the shard plan. The optional
`layer_pipeline=true` requests contiguous layer segments per worker so
activation order can be preserved. The explicit internal
`model.cluster.dispatch` service can send typed, encrypted assignments to a
paired worker when the worker listener and session key are available. The
worker acknowledges assignments, heartbeats and cancellations.
`model.cluster.recover` can resend the updated assignments after a reported
node loss, and `model.cluster.cancel` propagates cancellation when supplied
the session-key secret name.

`aos-modeld` binds the LAN listener only when the cluster is enabled and the
session key is available from the secret vault. Assignment loading is lazy:
weight staging can be acknowledged before a model context exists, while
token-level prefill and decode load llama.cpp on demand. `Prefill`, `Decode`, token, explicit text
inference and KV-page messages are defined, encrypted and bounded.
`model.cluster.infer_chat` lets an authorized caller send a prompt to the
selected node and receive generated text plus metrics; it requires
`allow_sensitive_data=true`, a valid session secret and a paired node.
An authenticated cancellation received on a separate connection is propagated
to the generation context and stops the work at a token boundary.
`model.cluster.kv_transfer` can export sequence-0 KV state from one worker,
carry it as encrypted CBOR pages of at most 512 KiB, and restore it on another
paired worker. The state is capped at 64 MiB and pages must arrive in order.
`model.cluster.weight_transfer` can explicitly transfer one bounded weight
range for a declared shard to the target worker's local staging area. Pages are
encrypted, ordered and atomically published with a manifest. A complete
manifest is reassembled as a GGUF and consumed by the independent Akasha
layer adapter; incomplete coverage remains non-executable.
`model.cluster.stage_local_model` automates the same operation for the local
GGUF before a pipeline. Layer segments transfer only the header/index and the
required `blk.N` tensor ranges; an empty range list keeps full-file staging.
The protocol also defines bounded
typed activation pages and result pages for the hop between contiguous layer
segments. The worker adapter executes those layers without depending on
llama.cpp RPC symbols; unsupported requests return an explicit Nack and can
fall back locally.
The daemon probes `llama_supports_rpc()` and distinguishes a llama.cpp build
without RPC from one with RPC but without the Akasha adapter. In both cases an
activation page is rejected cleanly until the complete encrypted path exists.
Retrying a shard accepts identical bytes and rejects different content without
overwriting the existing file. Publication is serialized across connections in
one daemon to preserve coverage entries. Use one daemon per staging directory.

Implicit `model.infer` routing remains local. Layer partitioning is available
through the explicit Akasha pipeline; native llama.cpp shard loading remains a
separate backend capability. Explicit KV transfer is available through
`model.cluster.kv_transfer`. By default, no model or prompt data leaves the
machine.

Auto-discovery takes effect when `aos-modeld` starts with both the LAN cluster
and auto-discovery enabled. It uses bounded UDP CBOR advertisements on the
configured local port and does not transmit prompts, model weights, tokens or
session-key values.

The **Run LAN test (1 token)** button in **Settings → Models** partitions the
declared layers contiguously across the paired workers, stages the local GGUF
to each worker, and runs the complete explicit pipeline. It uses one synthetic
token and never sends conversation content.

## YAML inventory (optional)

The same nodes may be seeded in the model daemon configuration:

```yaml
lan_cluster:
  enabled: false
  local_node_id: local
  listen_address: 127.0.0.1:9001
  session_key_secret: lan_cluster_session_key
  nodes:
    - node_id: worker-1
      display_name: Office worker
      address: 192.168.1.20:9001
      public_key_fingerprint: sha256:replace-with-verified-fingerprint
      trust: unpaired
      capabilities: []
```

Keep `enabled: false` until the pairing and worker-side validation have been
tested on the intended LAN.
