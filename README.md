# cal-based

A Windows-first calendar experiment built from the low-level capabilities in [`based`](https://github.com/xpjb/based).

![Calendar](gallery/calendar.png)

![Custom editor](gallery/editor.png)

## Calendar behavior

- One Sunday–Saturday week per row.
- Every day has an editable description.
- Events span inclusive date ranges and render as rounded strips along the bottom of each week.
- Overlapping events receive stable, bottom-up lanes. The first four lanes render.
- Type, spacing, hit areas, and editor controls scale with the Windows display scale.
- Click a day or event strip to open its description editor.
- Long-press a day, then drag and release to create an event without opening the editor.
- Long-press either end of an event, then drag to resize it.
- Drag an endpoint past the other endpoint and release to delete the event.
- Use the mouse wheel or trackpad to move through continuous weeks. **Today** eases back to the current date with one-pole motion; manual input stops the motion immediately.

The editor is a custom `based` widget. It stores text in a `ropey::Rope` and uses Sanscale's own layout for wrapping, hit testing, selection, caret motion, and rendering. `Ctrl+Enter` saves and `Escape` cancels.

## State design

All mutation goes through `CalendarTxn` and `LazyState::transact`. The Windows application uses `LocalCalendarState`, which applies a transaction locally and writes JSON. `AppModel` keeps the UI mirror, applies a successful transaction as client prediction, and requests the affected value for confirmation.

The stored model corresponds to:

```text
/year/month/day/description
/year/month/day/events/<event_id>
/events/<event_id>/start_date
/events/<event_id>/end_date
/events/<event_id>/description
```

Day-event memberships are rebuilt from the global event records when the file opens, so redundant paths cannot drift.

The default file is under the Windows local application-data directory. Set `CAL_BASED_DATA` to use a specific JSON file.

## Architecture

The application composes only these low-level `based` services:

- `Window`
- `Wgpu`
- `Sanscale`
- `Input`

Every application widget receives one concrete `AppContext`, including `&mut AppModel`. The model, transaction vocabulary, interval-lane assignment, and persistence code remain platform-neutral. A later Android host can reuse them without changing calendar semantics.

Networking and tailnet replication are intentionally not implemented in this experiment.

## Build and check

This repository pins a private `based` revision, so the build requires repository access and working GitHub SSH credentials.

```sh
./scripts/check.sh
./scripts/build-windows.sh
```

The release script writes:

```text
dist/cal-based-0.1.1-windows-x64.exe
dist/cal-based-0.1.1-windows-x64.exe.sha256
```

Rust checks use the managed Cargo binary, Nextest for behavioral tests, and `cargo xwin` for the Windows target. Clippy is not part of this project workflow.
