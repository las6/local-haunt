# Local Haunt

A little ghost for the servers you left behind.

Local Haunt is a macOS menu bar utility for finding listening TCP ports, identifying their projects and processes, and stopping servers that linger after their terminals close.

Created by **[las6](https://las6.net)**. Built with Rust, GPUI Kit, and gpui-tray as a personal learning project.

## Features

- Compact port table with process names, PIDs, and listening addresses.
- Inline search by port, process, project/app name, or PID; Cmd+F focuses the field.
- Aligned process details with full-value tooltips and click-to-copy values.
- Project identification from configured folders, nearby `package.json` files, and Git roots.
- Application and system-service identification from executable paths, including Local and BrowserStack.
- Single services shown directly; related services and worker processes grouped with expandable details.
- HTTP links, Copy URL, and Open app where an owning application can be identified.
- Stop using SIGTERM, or Force Kill using SIGKILL, with inline confirmation and process-identity checks.
- Automatic refresh every ten seconds while the window is focused, plus manual Refresh.
- Brief fade transitions when listeners appear or exit.
- Cmd/Ctrl +, −, and 0 to enlarge, reduce, or reset text size.
- Persistent project folders, zoom, and Auto refresh preferences.
- Native About panel with version, author credit, and website link; it opens independently of the port window.

The app stays in the menu bar when its window is closed. Opening the window brings it into the Dock and Cmd+Tab; closing the last window returns it to menu bar mode. Use the ghost menu or Cmd+Q to quit.

## Run and package

Requires macOS 15 or newer, Rust/Cargo, and Apple's command-line developer tools. The current builds target the architecture of the Mac that compiles them.

From the project directory:

```sh
# Debug app bundle, launched and attached to the terminal.
bash scripts/package-macos.sh --debug --run

# Optimized release app bundle.
bash scripts/package-macos.sh
```

In Zed, use **Local Haunt: build and run macOS app** or **Local Haunt: package release app**. Rerunning cancels the previous Zed task. After successful packaging, `--run` also stops any remaining instances using the exact executable path for this checkout and profile, waits up to five seconds for them to exit, then launches the new build. Installed releases and other checkouts are left alone. If an instance will not exit, the script reports an error instead of launching another copy.

The bundles are generated at:

- `target/app/debug/Local Haunt.app`
- `target/app/release/Local Haunt.app`

Plain `cargo run` works for development but does not load the packaged app's name, Dock icon, or About credits. `cargo build --release` produces an optimized executable; the packaging script creates the macOS `.app`, generates its icon, sets its version from Cargo, and ad-hoc signs it. Public distribution would need separate Developer ID signing and notarization.

## Settings

Open **Settings…** from the titlebar, the native app menu, or Cmd+comma. Settings replaces the port view; **← Ports** or Escape returns while retaining the list's filter and expanded rows.

Use **Add folders…** to choose project folders with the native macOS picker. Removing a folder only changes classification; it never deletes files. New installations start with no project folders configured.

Preferences are saved automatically to:

```text
~/Library/Application Support/Local Haunt/settings.json
```

The JSON fields are `project_roots` (absolute folder paths), `zoom` (−2 to 6 relative to 12px), and `auto_refresh` (boolean). Missing fields use defaults. Invalid settings produce an error and are left untouched; repair the file and reopen the window to retry. Settings stay outside the repository and app bundle.

## Identification and process controls

Project folders take precedence for development processes, and the most specific configured folder wins when scopes overlap. Path component matching avoids confusing similarly named folders. Group identities include paths so projects with the same package name remain separate.

Local's nginx, MySQL, and Mailpit services are identified through Local's app/service directories rather than process names. Application helpers belong to their outer `.app` bundle. Processes without enough path evidence remain visible under **Unidentified**.

Multiple workers with the same project/app, port, process, executable, and working directory share an endpoint row. Details retain every PID. Stop and Force Kill apply to the processes shown for that endpoint, rechecking start time and command before signalling. Permission failures are displayed. Shared services may affect more than one project, and supervisors can restart a stopped process.

HTTP assumes `http://localhost:port`; databases and other services may not serve web pages. Open app uses the identified application bundle; it cannot infer which terminal originally launched Node. Individual Local sites, process ancestry, and UDP listeners are not yet tracked.

## Code and styling

- `src/main.rs`: app lifecycle, menus, window creation, and packaging commands.
- `src/port_list.rs`: table, Settings view, polling, transitions, and process controls.
- `src/ports.rs`: macOS `lsof` queries, project/app classification, and signalling.
- `src/settings.rs`: preference validation, loading, and atomic file replacement.
- `src/theme.rs`: semantic colors, font, dimensions, zoom, and timing.
- `src/macos.rs`: Dock activation policy and the native About panel.
- `src/icon_export.rs`: generates icon PNGs from the shared ghost SVG.
- `assets/Credits.html`: author and website shown in the native About panel.

Change `ACCENT` in `src/theme.rs` to recolor the UI and all ghosts. Packaging generates the Dock/Cmd+Tab icon from the same compiled artwork. Theme values are named by role rather than color, and selected surfaces derive from the accent. The interface uses the macOS system UI font with a 12px base size. `WINDOW_TINT_OPACITY` controls the dark tint over the native window blur; `ELEMENT_TINT_OPACITY` independently controls backgrounds for filters, inputs, panels, and button hover states. Lower values reveal more of the backdrop without fading text or icons.

The compact table takes cues from [Zed's list component](https://github.com/zed-industries/zed/blob/main/crates/ui/src/components/list/list_item.rs). The learning goal is to explore Rust, native UI, and operating-system integration through small, understandable changes.

## Development checks

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Tests cover socket discovery, parsing, path classification, worker grouping, settings persistence, view navigation, refresh transitions, and SIGTERM/SIGKILL against test-owned child processes.

`vendor/gpui-tray` contains version 0.1.4 with two macOS fixes: an 18-point logical menu bar icon size, and clearing native menu tracking/highlight before replacing a menu. See `vendor/gpui-tray/LOCAL_PATCH.md` for maintenance notes. Cargo selects it with `[patch.crates-io]`.

The existing `block v0.1.6` future-compiler notice comes from GPUI's dependency chain.

References: [GPUI Kit](https://gpui-kit.com/docs/getting-started/), [gpui-tray](https://docs.rs/gpui-tray/0.1.4/gpui_tray/).
