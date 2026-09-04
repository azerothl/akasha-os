# Device capture (issue #137)

The first device slice exposes four IPC intents:

- `device.enumerate`
- `device.camera.capture`
- `device.mic.capture`
- `device.capture.stop`

Capture permissions are separate capabilities: `device.camera.capture`,
`device.camera.stream`, `device.mic.capture`, and `device.mic.stream`. A
persistent grant is stored as an agent + exact device + exact action tuple.
Revoking that tuple stops matching streams synchronously. `Allow once` is
request-scoped; `Always` is the only choice that is persisted.

Media is written only below
`var/sessions/<session>/devices/`. IPC returns an opaque capture id, the
service-generated artifact path, and metadata. Audit entries contain no media
bytes. Duration (60 s), size (50 MiB), and per-session count (32) are hard
upper bounds; streams stop automatically at their quota.

The policy gate runs before the backend is opened. Low trust is denied;
medium trust requires confirmation; high trust is automatic only for a
matching persistent capability. Windows uses the Media Foundation source
reader through COM. Linux and macOS return `UnsupportedPlatform` in slice 1;
the injectable fake backend is used by portable CI tests.

Manual Windows validation must be performed on Windows 10/11 with one camera
and one microphone. Confirm in Akasha OS before accepting the Windows OS
permission prompt, check `Caps` and `Audit`, capture once, start/stop a stream,
revoke the exact permission, and verify that the stream stops immediately.
