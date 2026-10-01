# Local macOS fixes

Based on gpui-tray 0.1.4 (Apache-2.0).

- Set NSImage logical size to 18 points high, preserving source pixels and aspect ratio.
- Cancel native menu tracking and clear the status button highlight before refreshing the menu after an action.

Remove the Cargo patch when an upstream release provides these fixes.
