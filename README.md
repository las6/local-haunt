# Local Haunt

A personal macOS menu bar utility for finding local development servers and the processes that linger after their terminals close.

Working name: Local Haunt.

## Learning goals

Learn Rust through small, explained steps, drawing comparisons with TypeScript and familiar web development tools. Explore where Rust makes sense for native apps, command-line utilities, and operating-system integration.

## Current milestone: menu bar integration

For the full app name and icon, use Zed's **Local Haunt: build and run macOS app** task (`task: spawn`), or run:

```sh
cd ~/Personal/local-haunt
bash scripts/package-macos.sh --debug --run
```

The packaging command creates `target/app/debug/Local Haunt.app` with the ghost app icon and ad-hoc signs it locally. It keeps the launched app attached to the task terminal. Plain `cargo run` still works for code experiments, but does not use the app bundle metadata.

Local Haunt stays out of the Dock and Cmd+Tab while it has no windows. Opening the greeting switches the app to regular mode, with its name and icon in the Dock and Cmd+Tab. Closing the last window returns to menu bar only mode. Cmd+Q quits while the window is active.

Build a release app for local use with:

```sh
bash scripts/package-macos.sh
```

The result is `target/app/release/Local Haunt.app`. Local ad-hoc signing is not Developer ID signing or notarization for public distribution.

The app starts with a purple ghost icon in the macOS menu bar and no window. Click it and choose **Open Local Haunt** to show the 480 × 320 greeting window. Choosing Open again activates the existing window. Closing the window leaves the app running; choose **Quit Local Haunt** in the menu to exit, or stop the process in the terminal.

The transparent title bar is retained from the greeting-window exercise. This version opens an ordinary window, not an anchored popover. Port discovery is the next feature after verifying this integration.

### Reading the code

- `Greeting` and its `Render` implementation still describe the greeting interface.
- `actions!(...)` declares the Open and Quit commands; `cx.on_action(...)` connects them to functions.
- `Tray::builder()` configures the native menu bar item. `gpui-tray` uses its `gpui-kit` feature to match our framework's types.
- `MenuBar` retains the tray handle in application-wide storage, keeping the icon alive.
- `src/macos.rs` switches AppKit between accessory and regular activation policies. `assets/Info.plist` gives the bundle its name, identifier, app icon, and menu bar startup behavior.
- `QuitMode::Explicit` keeps the event loop running after the greeting window closes.
- `open_greeting` defers window work until action dispatch finishes. `show_greeting` activates an existing window or creates one if none is open.
- `assets/ghost.svg` is a placeholder icon embedded into the executable with `include_bytes!`; it is not a separate runtime file dependency.
- `Cargo.lock` records resolved dependency versions; `target/` contains generated build output.

The earlier CLI exercise remains runnable with `cargo run --example hello_cli`.

### Validation

The menu bar implementation builds successfully. A temporary app bundle was launched for inspection, but the UI inspection tool could not access it reliably. The active-window regression test now passes, including retaining the existing window and closing then reopening. Icon size, native menu highlight dismissal, and Quit still need an interactive check in Zed or from the terminal.

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
