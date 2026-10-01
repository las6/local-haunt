use gpui_kit::*;
use gpui_tray::{Icon, Tray};

mod icon_export;
mod macos;
mod settings;
mod theme;

mod port_list;
mod ports;

use port_list::PortList;

// Actions are named commands that our native menu can dispatch.
actions!(local_haunt, [OpenPorts, About, Quit]);

// Keeping the handle here keeps the native menu bar item alive.
struct MenuBar(Tray);
impl Global for MenuBar {}

fn main() {
    // A packaging operation, handled before GPUI starts or a tray item is created.
    let mut arguments = std::env::args_os().skip(1);
    let argument = arguments.next();
    if argument.as_deref() == Some(std::ffi::OsStr::new("--version")) {
        println!("{}", env!("CARGO_PKG_VERSION"));
        return;
    }
    if argument.as_deref() == Some(std::ffi::OsStr::new("--export-iconset")) {
        let Some(directory) = arguments.next() else {
            eprintln!("--export-iconset requires an output directory");
            std::process::exit(2);
        };
        if let Err(error) = icon_export::export_iconset(std::path::Path::new(&directory)) {
            eprintln!("Could not export the app icon: {error}");
            std::process::exit(1);
        }
        return;
    }

    application().with_quit_mode(QuitMode::Explicit).run(|cx| {
        init(cx);
        macos::set_window_presence(false);
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                // Switch after GPUI has finished processing the close event.
                cx.defer(|cx| {
                    if cx.windows().is_empty() {
                        macos::set_window_presence(false);
                    }
                });
            }
        })
        .detach();
        cx.on_action(open_ports).on_action(about).on_action(quit);
        cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);
        cx.set_menus(vec![Menu::new("Local Haunt").items([
            MenuItem::action("About Local Haunt", About),
            MenuItem::separator(),
            MenuItem::action("Settings…", port_list::OpenSettings),
            MenuItem::separator(),
            MenuItem::action("Quit Local Haunt", Quit),
        ])]);

        let image = theme::ghost_image();
        let icon = Icon::from_gpui(&image, cx).expect("Failed to load the menu bar icon");

        let tray = Tray::builder()
            .icon(icon)
            .tooltip("Local Haunt")
            .menu(|_| {
                vec![
                    MenuItem::action("Open Local Haunt", OpenPorts),
                    MenuItem::action("About Local Haunt", About),
                    MenuItem::separator(),
                    MenuItem::action("Quit Local Haunt", Quit),
                ]
            })
            .build(cx)
            .expect("Failed to create the menu bar item");

        cx.set_global(MenuBar(tray));
        println!("Local Haunt is running. Click the ghost in the menu bar.");
    });
}

fn open_ports(_: &OpenPorts, cx: &mut App) {
    // A menu action can run while its active window is already being updated.
    // Wait until action dispatch releases that window before touching it.
    cx.defer(show_ports);
}

fn show_ports(cx: &mut App) {
    macos::set_window_presence(true);
    cx.activate(true);

    // Reuse the existing port window instead of opening another one.
    if let Some(window) = cx.windows().first().copied() {
        if let Err(error) = window.update(cx, |_, window, _| window.activate_window()) {
            eprintln!("Failed to activate the port window: {error}");
        }
        return;
    }

    let window_bounds = WindowBounds::centered(size(px(780.0), px(520.0)), cx);
    cx.open_window(
        WindowOptions {
            window_bounds: Some(window_bounds),
            window_background: WindowBackgroundAppearance::Blurred,
            titlebar: Some(TitlebarOptions {
                title: Some("Local Haunt".into()),
                appears_transparent: true,
                ..Default::default()
            }),
            ..Default::default()
        },
        |window, cx| cx.new(|cx| PortList::new(window, cx)),
    )
    .expect("Failed to open the Local Haunt window");
    println!("Opened the port window.");
}

fn about(_: &About, cx: &mut App) {
    cx.defer(|cx| {
        // About is an AppKit panel, independent of the GPUI port window.
        cx.activate(true);
        macos::show_about();
    });
}

fn quit(_: &Quit, cx: &mut App) {
    let tray = cx.global::<MenuBar>().0.clone();
    if let Err(error) = tray.close(cx) {
        eprintln!("Failed to remove the menu bar item: {error}");
    }
    cx.quit();
}

#[cfg(test)]
mod tests {
    use super::{About, OpenPorts, about, open_ports, show_ports};
    use gpui_kit::TestAppContext;

    #[gpui_kit::test]
    fn about_does_not_create_or_replace_the_port_window(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        cx.update(|cx| about(&About, cx));
        cx.run_until_parked();
        cx.update(|cx| assert!(cx.windows().is_empty()));
        cx.update(show_ports);
        let original = cx.update(|cx| cx.windows()[0]);
        cx.update(|cx| {
            original.update(cx, |_, _, cx| about(&About, cx)).unwrap();
        });
        cx.run_until_parked();
        cx.update(|cx| {
            assert_eq!(cx.windows().len(), 1);
            assert_eq!(cx.windows()[0].window_id(), original.window_id());
        });
    }

    #[gpui_kit::test]
    fn opening_from_an_active_window_reuses_it(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        cx.update(show_ports);
        let original = cx.update(|cx| cx.windows()[0]);

        // Reproduce menu action dispatch while the window is on GPUI's stack.
        cx.update(|cx| {
            original
                .update(cx, |_, _, cx| open_ports(&OpenPorts, cx))
                .unwrap();
        });
        cx.run_until_parked();
        cx.update(|cx| {
            assert_eq!(cx.windows().len(), 1);
            assert_eq!(cx.windows()[0].window_id(), original.window_id());
        });

        cx.update(|cx| {
            original
                .update(cx, |_, window, _| window.remove_window())
                .unwrap();
        });
        cx.update(|cx| assert!(cx.windows().is_empty()));
        cx.update(|cx| open_ports(&OpenPorts, cx));
        cx.run_until_parked();
        cx.update(|cx| assert_eq!(cx.windows().len(), 1));
    }
}
