# USB I/O (issue #137, slice 3)

Opt-in USB access for Preview agents. Enumeration and constrained I/O are
separate from camera/microphone capture.

## IPC intents

- `device.usb.enumerate`
- `device.usb.open`
- `device.usb.read`
- `device.usb.write`
- `device.usb.close`

Agents call the same intents as tools. Open requires confirmation (Allow once /
Always / Deny) under the trust gate. Persistent grants are per agent + exact
device + `device.usb.io`. Revoking closes matching handles synchronously.

## Capabilities

| Capability | Scope |
|------------|-------|
| `device.usb.io` | open, read, write, close |

`device.usb.enumerate` has no required cap (like `device.enumerate`).

## Platform backends

| OS | Status | Notes |
|----|--------|-------|
| Windows | Supported | SetupAPI enumeration; serial (COM) open/read/write |
| Linux | `UnsupportedPlatform` | follow-up slice |
| macOS | `UnsupportedPlatform` | follow-up slice |

Device ids are opaque but stable per enumeration, for example
`win:Serial:COM3` or `win:Usb:1234:5678:0`.

## Quotas and audit

- Read/write capped at 4 KiB per call
- I/O timeout default 5 s (configurable per request, hard-capped)
- Max 8 open handles per agent, 32 global
- Audit records counts and handle ids only — no bulk payload dumps

## Grant chrome (Designer)

Confirmation UI mirrors device capture with neutral keys:

| Key (EN) | Key (FR) | Buttons |
|----------|----------|---------|
| USB | USB | Allow once · Always · Deny |

The control bar shows `device_usb` label when `action.kind` is `device.usb.io`.

## Manual validation (Windows)

1. Plug a USB-serial adapter (or virtual COM port).
2. Delegate an agent with `device.usb.io` and run `device.usb.enumerate`.
3. Confirm Akasha grant chrome (Allow once / Always / Deny).
4. `device.usb.open` with a `win:Serial:*` device id, then read/write/close.
5. Verify Audit (`device.usb.open.request`, `opened`, `read`, `write`, `close`)
   without raw bytes; revoke in Caps stops open handles.

Linux/macOS CI uses `FakeUsbIoBackend`; host enumeration returns
`UnsupportedPlatform` until a follow-up slice lands.
