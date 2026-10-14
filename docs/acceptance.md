# Acceptance

Accepted on October 14, 2026.

## Automated checks

- Six platform-neutral model tests pass through Nextest.
- The library passes the native compiler check.
- The application passes `cargo xwin check` for `x86_64-pc-windows-msvc`.
- The optimized Windows build produces a valid x64 PE executable.

## Windows-host smoke checks

The release executable was run under Wine with WGPU's Vulkan backend.

Verified:

- launch, first frame, resizing, and continuous week rendering;
- Sunday-first columns and day descriptions;
- deterministic bottom-up event lanes and four-lane rendering;
- day editor rendering, keyboard replacement, save, and JSON persistence;
- event editor rendering, keyboard replacement, save, and JSON persistence;
- long-press day drag with a live event preview;
- release creates the inclusive event range without opening the editor;
- long-press endpoint drag updates the live range preview;
- crossing the fixed endpoint deletes the event on release;
- the final persisted file remains readable after each operation.

The Wine host emitted software-display EGL warnings because the headless X server had no accelerated DRI device. Rendering and interaction still completed correctly.

## Deferred scope

- Android host and packaging.
- Tailnet transport and replicated state implementation.
- Native Windows hardware acceptance outside Wine.
