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
4. Open **Settings → Secrets vault**, enter a random 32-byte key as exactly 64
   hexadecimal characters, and save it under that secret name. The value is
   write-only in the UI.
5. In the LAN cluster section, enter the remote node ID, display name, LAN
   address (`192.168.x.y:port`, loopback and link-local addresses are also
   accepted), and its public-key fingerprint.
6. Select **Add node**, or wait for a discovery candidate, then select
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

`model.cluster.plan` computes the shard plan. The explicit internal
`model.cluster.dispatch` service can send typed, encrypted assignments to a
paired worker when the worker listener and session key are available. The
worker acknowledges assignments, heartbeats and cancellations.
`model.cluster.recover` can resend the updated assignments after a reported
node loss, and `model.cluster.cancel` propagates cancellation when supplied
the session-key secret name.

`aos-modeld` binds the LAN listener only when the cluster is enabled and the
session key is available from the secret vault. Actual weight loading, shard
execution and token routing still require model-engine integration. `Prefill`,
`Decode`, token and KV-page messages are defined, encrypted and bounded; they
return an explicit `Nack` until that executor is installed. By default, no
model or prompt data leaves the machine.

Auto-discovery takes effect when `aos-modeld` starts with both the LAN cluster
and auto-discovery enabled. It uses bounded UDP CBOR advertisements on the
configured local port and does not transmit prompts, model weights, tokens or
session-key values.

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
