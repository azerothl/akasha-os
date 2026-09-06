# Device capture (issue #137)

The first device slice exposes four IPC intents:

- `device.enumerate`
- `device.camera.capture`
- `device.mic.capture`
- `device.capture.stop`

Agents call the same intents as tools. A one-shot camera capture is encoded as
PNG (`image/png`) so a vision chat model can describe the frame on the next
turn. Microphone clips stay as PCM in this slice; always-on STT is out of
scope.

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
matching persistent capability.

## Platform backends

| OS | Camera | Microphone | Library |
|----|--------|------------|---------|
| Windows | Media Foundation (COM) | Media Foundation | `WindowsMediaFoundationBackend` |
| Linux | V4L2 | ALSA/PulseAudio/JACK via cpal | `HostDeviceCaptureBackend` (`nokhwa` + `cpal`) |
| macOS | AVFoundation | CoreAudio via cpal | `HostDeviceCaptureBackend` (`nokhwa` + `cpal`) |

Device ids are opaque but stable per enumeration:
`{platform}:{Camera|Microphone}:{index}` (for example `linux:Camera:0`).

OS permission prompts are surfaced by the host APIs. When the OS denies access,
capture returns `OsPermissionDenied` with a human-readable message. Linux
typically has no TCC-style preflight; macOS requires Camera/Microphone consent
in System Settings. The injectable fake backend is used by portable CI tests.

Manual validation:

- **Windows 10/11** — one camera and one microphone; confirm Akasha grant
  chrome before accepting the Windows permission prompt.
- **Linux** — V4L2 camera (`/dev/video*`) and PulseAudio/ALSA input; grant
  Akasha confirmation, capture once, start/stop a stream, revoke in Caps.
- **macOS** — AVFoundation camera and CoreAudio mic; grant macOS TCC when
  prompted, then repeat the same Akasha flow as Windows.

Check `Caps` and `Audit`, verify artifacts under
`var/sessions/<session>/devices/`, and confirm revocation stops streams
immediately.
