use gpui_kit::*;
use gpui_tray::{Icon, Tray};

mod macos;

// This view has no state yet; it only describes what to draw.
struct Greeting;

impl Render for Greeting {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .size_full()
            .items_center()
            .justify_center()
            .bg(rgb(0x20242b))
            .text_color(rgb(0xffffff))
            .child("Hello from Local Haunt!")
    }
}

// Actions are named commands that our native menu can dispatch.
actions!(local_haunt, [OpenGreeting, Quit]);

// Keeping the handle here keeps the native menu bar item alive.
struct MenuBar(Tray);
impl Global for MenuBar {}

fn main() {
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
        cx.on_action(open_greeting).on_action(quit);
        cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);
        cx.set_menus(vec![
            Menu::new("Local Haunt").items([MenuItem::action("Quit Local Haunt", Quit)]),
        ]);

        let image = Image::from_bytes(
            ImageFormat::Svg,
            include_bytes!("../assets/ghost.svg").to_vec(),
        );
        let icon = Icon::from_gpui(&image, cx).expect("Failed to load the menu bar icon");

        let tray = Tray::builder()
            .icon(icon)
            .tooltip("Local Haunt")
            .menu(|_| {
                vec![
                    MenuItem::action("Open Local Haunt", OpenGreeting),
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

fn open_greeting(_: &OpenGreeting, cx: &mut App) {
    // A menu action can run while its active window is already being updated.
    // Wait until action dispatch releases that window before touching it.
    cx.defer(show_greeting);
}

fn show_greeting(cx: &mut App) {
    macos::set_window_presence(true);
    cx.activate(true);

    // Reuse the existing greeting window instead of opening another one.
    if let Some(window) = cx.windows().first().copied() {
        if let Err(error) = window.update(cx, |_, window, _| window.activate_window()) {
            eprintln!("Failed to activate the greeting window: {error}");
        }
        return;
    }

    let window_bounds = WindowBounds::centered(size(px(480.0), px(320.0)), cx);
    cx.open_window(
        WindowOptions {
            window_bounds: Some(window_bounds),
            titlebar: Some(TitlebarOptions {
                title: Some("Local Haunt".into()),
                appears_transparent: true,
                ..Default::default()
            }),
            ..Default::default()
        },
        |_, cx| cx.new(|_| Greeting),
    )
    .expect("Failed to open the Local Haunt window");
    println!("Opened the greeting window.");
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
    use super::{OpenGreeting, open_greeting, show_greeting};
    use gpui_kit::TestAppContext;

    #[gpui_kit::test]
    fn opening_from_an_active_window_reuses_it(cx: &mut TestAppContext) {
        cx.update(show_greeting);
        let original = cx.update(|cx| cx.windows()[0]);

        // Reproduce menu action dispatch while the window is on GPUI's stack.
        cx.update(|cx| {
            original
                .update(cx, |_, _, cx| open_greeting(&OpenGreeting, cx))
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
        cx.update(|cx| open_greeting(&OpenGreeting, cx));
        cx.run_until_parked();
        cx.update(|cx| assert_eq!(cx.windows().len(), 1));
    }
}
