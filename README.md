# Local Haunt

A personal macOS menu bar utility for finding local development servers and the processes that linger after their terminals close.

Working name: Local Haunt.

## Learning goals

Learn Rust through small, explained steps, drawing comparisons with TypeScript and familiar web development tools. Explore where Rust makes sense for native apps, command-line utilities, and operating-system integration.

## Current milestone: first release candidate

For the full app name and icon, use Zed's **Local Haunt: build and run macOS app** task (`task: spawn`), or run:

```sh
cd ~/Personal/local-haunt
bash scripts/package-macos.sh --debug --run
```

The packaging command creates `target/app/debug/Local Haunt.app` with the ghost app icon and ad-hoc signs it locally. It keeps the launched app attached to the task terminal. Plain `cargo run` still works for code experiments, but does not use the app bundle metadata.

Local Haunt stays out of the Dock and Cmd+Tab while it has no windows. Opening the port window switches the app to regular mode, with its name and icon in the Dock and Cmd+Tab. Closing the last window returns to menu bar only mode. Cmd+Q quits while the window is active.

Build a release app for local use with:

```sh
bash scripts/package-macos.sh
```

The result is `target/app/release/Local Haunt.app`. Local ad-hoc signing is not Developer ID signing or notarization for public distribution.

The app starts with a ghost icon in the macOS menu bar and no window. Click it and choose **Open Local Haunt** to show the 780 × 520 port window. Choosing Open again activates the existing window. Closing the window leaves the app running; choose **Quit Local Haunt** in the menu to exit, or stop the process in the terminal.

The transparent title bar is retained from the greeting-window exercise. This version opens an ordinary window, not an anchored popover. The window scans listening TCP sockets on opening and when you click Refresh. Each row shows the port, process, PID, addresses, and working directory when accessible. Project names come from the nearest package.json name or Git folder, with the working directory name as a fallback. The compact table has Name, Port, Process, PID, and Actions columns with alternating row backgrounds. Single endpoints appear directly as rows. Apps or projects with multiple related endpoints get a collapsed parent row that expands into service rows. Unidentified endpoints stay separate. Click a service row or its ⋯ action to show paths and other metadata. **All** shows projects, apps/system services (including Local), and unidentified listeners. **Projects** filters to your project folders. HTTP opens the localhost address using plain HTTP (some ports are databases or other non-web services). Copy URL is in the expanded details. Stop asks for inline confirmation, then sends SIGTERM to every process shown for that endpoint. Force Kill in the details uses SIGKILL with its own confirmation. The start time and command are rechecked before signalling; exited or changed processes are rejected. Permission failures are shown in a dismissible message. Shared services can affect more than one project. Open app in the details switches to the app identified by the executable bundle, or to Local for its services. This does not infer which terminal originally launched a Node process.

Auto is on by default, checks every ten seconds while the window is focused, and pauses when it is inactive. The window-owned polling task ends when the window closes. Manual Refresh remains available and Auto can be disabled. Existing rows are reconciled by service identity; new rows fade in and exited rows fade out for roughly 850ms before removal. Changes inside collapsed groups update their summaries rather than exposing child rows. GPUI animations respect macOS Reduce Motion.

The interface explicitly uses GPUI’s `.SystemUIFont` (macOS system UI font), with a 12px base size. Cmd/Ctrl + or = enlarges, − reduces, and 0 resets; bounds are 10–18px. Row heights, smaller labels, and table columns scale as well. Zoom and Auto refresh preferences persist across launches. The font remains defined by the theme. The header reserves 84px on the left for the native traffic lights.

Project folders are configured through **Settings → Add folders…** using the native macOS folder picker. Remove deletes only the folder from this list, not any files. New installations start with an empty list; app/system detection still works. Path component boundaries prevent folders such as `Personal-other` from matching. Project group identity uses the closest manifest/Git root or the first folder inside the scope, so unrelated projects with identical package names do not merge. Local is identified by its app/service paths, never simply by the name nginx or MySQL. Application bundles group related helper processes under the app name; system executables appear under Apps & System using their process names. BrowserStack’s ~/.browserstack/BrowserStackLocalApp path identifies BrowserStack. System and usr executable paths inside CoreSimulator runtime roots are classified as system services as well. Unmatched paths stay in Unidentified.

Working directories and the first executable `txt` entry are read from lsof. Multiple workers with the same group, port, process, executable, and directory fold into one endpoint row; the detail view retains all PIDs and addresses. Different services stay separate. These are path-based hints and do not yet identify individual Local sites or trace process ancestry.

### Reading the code

- `src/port_list.rs` describes the port window and runs scans on a background thread so the UI stays responsive.
- `src/ports.rs` queries macOS lsof, groups sockets by port and PID, and infers project names from working directories.
- `actions!(...)` declares the Open and Quit commands; `cx.on_action(...)` connects them to functions.
- `Tray::builder()` configures the native menu bar item. `gpui-tray` uses its `gpui-kit` feature to match our framework's types.
- `MenuBar` retains the tray handle in application-wide storage, keeping the icon alive.
- `src/macos.rs` switches AppKit between accessory and regular activation policies. `assets/Info.plist` gives the bundle its name, identifier, app icon, and menu bar startup behavior.
- `QuitMode::Explicit` keeps the event loop running after the port window closes.
- `open_ports` defers window work until action dispatch finishes. `show_ports` activates an existing window or creates one if none is open.
- `assets/ghost.svg` is a placeholder icon embedded into the executable and recolored through `theme::ghost_image`; it is not a separate runtime file dependency.
- `Cargo.lock` records resolved dependency versions; `target/` contains generated build output.

### Validation

The scanner tests cover IPv4/IPv6 grouping, distinct processes on the same port, paths with spaces and newlines, discovery of a real listening socket with its working directory, path-based classification, executable metadata, folding multiple workers without merging distinct services, retiring rows after refresh, and SIGTERM/SIGKILL against test-owned child processes (including rejection of stale identity). The window regression test covers repeated Open actions and closing then reopening. The earlier menu bar and Dock behavior was checked interactively.

Scanning includes all visible listening TCP sockets, including system apps and servers bound to all interfaces. Processes that exit during a scan or cannot be inspected may have no working directory. Project names are hints; the actual path is shown alongside them.

### Local tray fixes

`vendor/gpui-tray` contains version 0.1.4 with two small macOS fixes: an 18-point logical icon size, and cancellation of menu tracking plus clearing the button highlight before replacing a menu. `[patch.crates-io]` selects that local copy. See `vendor/gpui-tray/LOCAL_PATCH.md` for maintenance notes.

The active-window regression test exercises reopening while the original window is being updated, checks that its identity is preserved, and checks closing then reopening:

```sh
cargo test --bin local-haunt
```

The existing `block v0.1.6` future-compiler warning comes from GPUI's dependency chain.

References: [GPUI Kit](https://gpui-kit.com/docs/getting-started/), [gpui-tray](https://docs.rs/gpui-tray/0.1.4/gpui_tray/).

## Initial app scope

- List listening TCP ports, process names, and PIDs.
- Identify projects from process working directories, nearby package.json files, and Git roots.
- Show the actual path alongside any inferred project name.
- Open web services in the browser and reveal project folders.
- Offer graceful stop and a separate force-stop action.

Begin with local Node development servers. Docker and Homebrew integrations can follow later.

## Development approach

Build in small stages with explanations of the Rust concepts each stage introduces. Keep the initial menu bar prototype separate from process termination so its behavior is easy to inspect.

The dense list design takes cues from [Zed’s list component](https://github.com/zed-industries/zed/blob/main/crates/ui/src/components/list/list_item.rs): disclosure arrows, restrained hover surfaces, and a consistent end slot for actions.

### Styling

`src/theme.rs` defines the palette, font, dimensions at each zoom level, and refresh/animation timing. `ACCENT` is the single brand color: links, selected backgrounds, detail borders, the refresh indicator, and the in-window/menu-bar ghost are derived from it. Palette fields describe semantic roles rather than specific colors; views use those roles. The packaging script generates the Dock/Cmd+Tab `.icns` from the compiled app’s `theme::ghost_svg()` using the same SVG renderer as GPUI, so the color follows `ACCENT` automatically. Icon PNGs are generated under `target/app/<profile>/local-haunt.iconset` and converted with macOS iconutil before signing. The original `assets/local-haunt.icns` is no longer used by packaging.

The title bar contains a small ghost beside the app name. Refresh keeps its label and layout fixed, using a reserved dot slot to indicate a scan. Auto polls every ten seconds while focused.

### Persistent settings

Project folder paths, zoom, and Auto refresh are stored in `~/Library/Application Support/Local Haunt/settings.json`. The file stays outside the repository and app bundle. Writes replace the file atomically. JSON fields are `project_roots` (absolute paths), `zoom` (−2 through 6 relative to 12px), and `auto_refresh` (boolean). Unknown fields are ignored and missing fields use defaults. Invalid JSON or relative paths show an error and are left untouched; repair the file and reopen the app to retry. Folder changes trigger a fresh scan, including when a previous scan is in progress.

Application bundles and known system/service paths continue to identify background apps. A listener outside the configured project folders remains an app/system entry if there is executable-path evidence, otherwise Unidentified. This avoids classifying every unrecognized process as background noise.
