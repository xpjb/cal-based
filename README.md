# cal-based

A Windows-first calendar experiment built from the low-level capabilities in [`based`](https://github.com/xpjb/based).

![Calendar](gallery/calendar.png)

![Custom editor](gallery/editor.png)

![Day color settings](gallery/settings.png)

## Calendar behavior

- One Sunday–Saturday week per row.
- Every day has an editable description.
- Events span inclusive date ranges and render as rounded strips along the bottom of each week.
- Overlapping events receive stable, bottom-up lanes. The first four lanes render.
- Type, spacing, hit areas, and editor controls scale with the Windows display scale.
- Past, today, and upcoming weekdays and weekends have six distinct colors. Use **Settings** to change any of them.
- Hover, pressed, and dragging states are visible on every interactive calendar surface.
- Click a day or event strip to open its description editor. Clicking outside the editor closes it.
- With a mouse, drag a day immediately to create an event and drag an event endpoint immediately to resize it.
- On touch, long-press a day or endpoint before dragging.
- Drag an endpoint onto or past the other endpoint and release to delete the event.
- Use the mouse wheel or trackpad to move through continuous weeks. **Today** eases back with the current week aligned to the top; manual input stops the motion immediately.

The editor is a custom `based` widget. It stores text in a `ropey::Rope` and uses Sanscale's own layout for wrapping, hit testing, selection, caret motion, and rendering. `Ctrl+Enter` saves and `Escape` cancels.

## State design

All reads and mutations go through the separate `StateClient::request` and `StateClient::transact` operations. Both return a request ID. The Windows application uses `LocalCalendarState`, which applies transactions locally, writes JSON, and sends each requested or affected value through `based::UserEventSender<StateEvent>`. There is no response polling.

`CalendarWidget` owns the current full `CalendarData` snapshot for now. State responses arrive as `Event::User` values and are routed through the widget tree. This keeps the client boundary in place without adding range subscriptions or cache tiers yet.

The stored model corresponds to:

```text
/year/month/day/description
/year/month/day/events/<event_id>
/events/<event_id>/start_date
/events/<event_id>/end_date
/events/<event_id>/description
/settings/day_colors/<past|today|upcoming>_<weekday|weekend>
```

Day-event memberships are rebuilt from the global event records when the file opens, so redundant paths cannot drift.

The default file is under the Windows local application-data directory. Set `CAL_BASED_DATA` to use a specific JSON file.

## Architecture

The application composes only these low-level `based` services:

- `Window`
- `Wgpu`
- `Sanscale`
- `Input`

Every application widget receives one concrete `AppContext`, including only `&mut dyn StateClient` for state access. The calendar widget owns its UI snapshot, while the backing store and event sender stay inside the host. The Windows host, calendar widget, editor widget, settings widget, reusable interactive surfaces, and GPU painter are separate modules. Buttons own their hover and press state. Calendar cells, events, endpoints, and text fields use the same explicit idle, hovered, pressed, and dragging surface states.

Rounded surfaces are single GPU quads. The fragment shader computes the radius, border, and antialias coverage, so controls do not depend on CPU polygon tessellation.

The model, transaction vocabulary, interval-lane assignment, and persistence code remain platform-neutral. A later Android host can reuse them without changing calendar semantics.

Networking and tailnet replication are intentionally not implemented in this experiment.

## Build and check

This repository pins a private `based` revision, so the build requires repository access and working GitHub SSH credentials.

```sh
./scripts/check.sh
./scripts/build-windows.sh
```

The release script writes:

```text
dist/cal-based-0.1.3-windows-x64.exe
dist/cal-based-0.1.3-windows-x64.exe.sha256
```

Rust checks use the managed Cargo binary, Nextest for behavioral tests, and `cargo xwin` for the Windows target. Clippy is not part of this project workflow.
