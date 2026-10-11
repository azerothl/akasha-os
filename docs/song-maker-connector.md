# Song Maker connector

Akasha OS can connect to Song Maker's native DeclUI host from **Settings →
Advanced → Song Maker**. This is an explicit, local connection; Akasha does not
probe the network or contact Song Maker on startup.

## Connect

1. In Song Maker, enable **local access** in Settings.
2. Copy the loopback address shown by Song Maker.
3. Paste that address in Akasha OS and select **Discover and load projects**.

Song Maker selects an available loopback port when its host starts. Copy the
current address again after restarting Song Maker if that port changes. Akasha
accepts only plain HTTP `localhost`, `127.0.0.1`, or `::1` URLs and only sends
requests after the user selects the connect button.

## Current capability

The connector checks `song-maker.host.v1`, verifies that the endpoint exposes
the `song-maker-music` API, then invokes `list_projects` only when the host
advertises that capability. Project names and update dates are shown in Akasha.
The current Song Maker host is read-only: generation, stem separation, mix
export, and style application remain in Song Maker and are not reported as
available by this connector.

The URL is saved in Akasha's local preferences. **Clear** removes the saved
address and the current connection result. A connection attempt has a short
timeout and does not block the interface.
