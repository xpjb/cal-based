# Acceptance

Accepted on October 15, 2026.

## Relevant checks

- The model was not changed, so the model test suite was not rerun for this UI-only pass.
- The application passes `cargo xwin check` for `x86_64-pc-windows-msvc`.
- The optimized Windows build produces a valid x64 PE executable.

## Windows-host smoke checks

The release executable was run under Wine with WGPU's Vulkan backend.

Verified:

- launch, first frame, resizing, and continuous week rendering;
- Sunday-first columns and day descriptions;
- larger calendar and editor type at 100% scale and simulated 200% Windows display scale;
- shader-antialiased rounded event strips, endpoint handles, Today button, editor panel, and editor controls;
- visible idle, hover, pressed, and dragging states for calendar and editor interactions;
- pointer cursor changes for links, text, day dragging, active dragging, and endpoint resizing;
- deterministic bottom-up event lanes and four-lane rendering;
- day editor rendering, keyboard replacement, save, and JSON persistence;
- event editor rendering, keyboard replacement, save, and JSON persistence;
- mouse day dragging enters the drag state after normal pointer slop with no hold delay;
- mouse endpoint resizing enters the drag state after normal pointer slop with no hold delay;
- touch retains long-press promotion at the gesture layer;
- release creates the inclusive event range without opening the editor;
- endpoint dragging updates the live range preview and crossing the fixed endpoint deletes the event;
- Today navigation produces distinct in-flight frames, aligns the current week at the top, and stops on manual wheel input;
- the final persisted file remains readable after each operation.

The Wine host emitted software-display EGL warnings because the headless X server had no accelerated DRI device. Rendering and interaction still completed correctly.

## Deferred scope

- Android host and packaging.
- Tailnet transport and replicated state implementation.
- Native Windows hardware acceptance outside Wine.
