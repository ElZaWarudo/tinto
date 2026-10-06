# Local provider patch

This directory vendors `tauri-plugin-wdio-webdriver` 1.2.0 from
`webdriverio/desktop-mobile` under its original MIT license.

The Linux screenshot implementation retains `SnapshotRegion::Visible` for
normal viewport screenshots and falls back to `SnapshotRegion::FullDocument`
when WebKitGTK rejects the visible region. WebKitGTK requires a mapped GTK
surface for that region, so the upstream implementation returns a
snapshot-creation error when a Tauri window is hidden from initial creation.
The fallback preserves the PNG WebDriver contract while remaining renderable in
the isolated background mode.

The provider also requires a second, internal nonce on every direct request.
The authenticated agent proxy injects this secret upstream; callers know only
their separate session nonce and cannot bypass the proxy through the raw
loopback port.

Pumarejo also patches keyboard action dispatch so Tab and Shift+Tab perform
cancellable sequential focus traversal, including open shadow roots, instead
of emitting a synthetic event that cannot trigger the browser default action.
The provider exposes a launch-scoped `pumarejo/window-capabilities` endpoint so
desktop actions are preflighted against the concrete window's resizable and
maximizable constraints, while mobile support is reported truthfully before
dispatch. Maximize also verifies its native postcondition before returning.

RU1 verifies the patch through the same live visible/background W3C sequence on
Windows and Ubuntu, including PNG signature validation, DOM actions, session
deletion, and process/port cleanup. Re-evaluate this patch before upgrading the
vendored provider.
