# Song-maker / mix assistant (reference module)

**License:** MIT (community tree)  
**Tracking:** [issue #432](https://github.com/azerothl/akasha-os/issues/432)  
**Contract:** [docs/mix-assistant.md](../../../docs/mix-assistant.md)

Path A stub: host-computed mix **descriptors** (JSON only), akasha-model
**preset catalogue**, confirmable `mix_proposal` batch, one-action undo.

- No WAV / stems in the guest or this tree
- No audio encoder
- Apply updates in-memory session settings; a DAW host is out of sandbox
- Caps `mix.read:*` / `mix.apply:*` fail-closed at install review

Not in the signed Preview catalogue by default — copy to
`var/modules/src/song-maker/` and package/install with cap review.
