use crate::ports::{self, Category, Group, Listener};
use crate::settings::Settings;
use crate::theme::{self, Metrics};
use gpui_kit::component::Sizable;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use std::collections::{BTreeMap, BTreeSet};

actions!(
    local_haunt,
    [
        ZoomIn,
        ZoomOut,
        ResetZoom,
        OpenSettings,
        BackToPorts,
        FocusSearch
    ]
);

// A server can have multiple workers listening on the same endpoint.
// Keep every PID in the details, but show that endpoint once in the list.
struct Endpoint<'a> {
    rows: Vec<&'a Listener>,
}
fn grouped(listeners: &[Listener]) -> BTreeMap<Group, Vec<Endpoint<'_>>> {
    let mut groups: BTreeMap<Group, BTreeMap<_, Vec<&Listener>>> = BTreeMap::new();
    for row in listeners {
        groups
            .entry(row.group.clone())
            .or_default()
            .entry((
                row.port,
                row.process.clone(),
                row.executable.clone(),
                row.directory.clone(),
            ))
            .or_default()
            .push(row);
    }
    groups
        .into_iter()
        .map(|(group, endpoints)| {
            (
                group,
                endpoints
                    .into_values()
                    .map(|rows| Endpoint { rows })
                    .collect(),
            )
        })
        .collect()
}

// Filter endpoints, not individual workers: actions must keep all of their targets.
fn filtered_groups<'a>(
    listeners: &'a [Listener],
    query: &str,
    projects_only: bool,
) -> BTreeMap<Group, Vec<Endpoint<'a>>> {
    let query = query.trim().to_lowercase();
    let query = query.strip_prefix(':').unwrap_or(&query);
    let mut groups = grouped(listeners);
    groups.retain(|group, endpoints| {
        if projects_only && group.category != Category::Projects {
            return false;
        }
        endpoints.retain(|endpoint| {
            query.is_empty()
                || group.label.to_lowercase().contains(query)
                || endpoint.rows.iter().any(|row| {
                    row.port.to_string().contains(query)
                        || row.pid.to_string().contains(query)
                        || row.process.to_lowercase().contains(query)
                        || row
                            .project
                            .as_ref()
                            .is_some_and(|name| name.to_lowercase().contains(query))
                })
        });
        !endpoints.is_empty()
    });
    groups
}

#[derive(Default)]
pub struct PortList {
    listeners: Vec<Listener>,
    scanning: bool,
    refresh_pending: bool,
    error: Option<String>,
    control_error: Option<String>,
    scan_task: Option<Task<()>>,
    expanded_groups: BTreeSet<String>,
    expanded_rows: BTreeSet<String>,
    projects_only: bool,
    search: Option<Entity<InputState>>,
    project_roots: Vec<std::path::PathBuf>,
    settings_path: Option<std::path::PathBuf>,
    settings_valid: bool,
    settings_open: bool,
    folder_task: Option<Task<()>>,
    auto_refresh: bool,
    active: bool,
    zoom: i8,
    focus: Option<FocusHandle>,
    ghost: Option<std::sync::Arc<Image>>,
    #[cfg_attr(test, allow(dead_code))]
    poll_task: Option<Task<()>>,
    transition_task: Option<Task<()>>,
    control_task: Option<Task<()>>,
    owner_task: Option<Task<()>>,
    stopping: bool,
    confirmation: Option<(Vec<Listener>, bool)>,
    added: BTreeSet<(u32, u16)>,
    removed: BTreeSet<(u32, u16)>,
    revision: usize,
    has_scanned: bool,
}

impl PortList {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        // Inputs and tooltips should use the same dark appearance as the port table.
        gpui_kit::component::Theme::change(gpui_kit::component::ThemeMode::Dark, Some(window), cx);
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        cx.bind_keys([
            KeyBinding::new("cmd-+", ZoomIn, Some("PortList")),
            KeyBinding::new("cmd-=", ZoomIn, Some("PortList")),
            KeyBinding::new("cmd--", ZoomOut, Some("PortList")),
            KeyBinding::new("cmd-0", ResetZoom, Some("PortList")),
            KeyBinding::new("cmd-,", OpenSettings, Some("PortList")),
            KeyBinding::new("cmd-f", FocusSearch, Some("PortList")),
            KeyBinding::new("escape", BackToPorts, Some("PortList")),
            KeyBinding::new("ctrl-+", ZoomIn, Some("PortList")),
            KeyBinding::new("ctrl-=", ZoomIn, Some("PortList")),
            KeyBinding::new("ctrl--", ZoomOut, Some("PortList")),
            KeyBinding::new("ctrl-0", ResetZoom, Some("PortList")),
        ]);
        let search =
            cx.new(|cx| InputState::new(window, cx).placeholder("Search ports, names, PIDs"));
        cx.subscribe(&search, |_, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                cx.notify();
            }
        })
        .detach();
        #[allow(unused_mut)]
        let mut view = Self {
            auto_refresh: true,
            settings_valid: true,
            active: window.is_window_active(),
            focus: Some(focus),
            search: Some(search),
            ghost: Some(theme::ghost_image()),
            ..Self::default()
        };
        #[cfg(not(test))]
        {
            view.settings_path = crate::settings::path();
            if let Some(path) = &view.settings_path {
                match Settings::load(path) {
                    Ok(settings) => {
                        view.project_roots = settings.project_roots;
                        view.zoom = settings.zoom;
                        view.auto_refresh = settings.auto_refresh;
                    }
                    Err(error) => {
                        view.control_error = Some(error);
                        view.settings_valid = false;
                    }
                }
            }
        }
        cx.observe_window_activation(window, |view, window, cx| {
            view.active = window.is_window_active();
            if view.active && view.auto_refresh {
                view.refresh(cx);
            }
        })
        .detach();
        #[cfg(not(test))]
        {
            view.refresh(cx);
            view.poll_task = Some(cx.spawn(async move |view, cx| {
                loop {
                    cx.background_executor()
                        .timer(theme::REFRESH_INTERVAL)
                        .await;
                    if view
                        .update(cx, |view, cx| {
                            if view.active && view.auto_refresh {
                                view.refresh(cx);
                            }
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            }));
        }
        view
    }

    fn save_settings(&mut self) {
        // Preserve malformed files for repair rather than silently overwriting them.
        if !self.settings_valid {
            return;
        }
        if let Some(path) = &self.settings_path {
            let settings = Settings {
                project_roots: self.project_roots.clone(),
                zoom: self.zoom,
                auto_refresh: self.auto_refresh,
            };
            if let Err(error) = settings.save(path) {
                self.control_error = Some(error);
            }
        }
    }

    fn add_folders(&mut self, cx: &mut Context<Self>) {
        let prompt = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: true,
            prompt: Some("Choose project folders".into()),
        });
        self.folder_task = Some(cx.spawn(async move |view, cx| {
            let result = prompt.await;
            let _ = view.update(cx, |view, cx| {
                match result {
                    Ok(Ok(Some(paths))) => {
                        for path in paths {
                            if !view.project_roots.contains(&path) {
                                view.project_roots.push(path);
                            }
                        }
                        view.save_settings();
                        view.refresh(cx);
                    }
                    Ok(Ok(None)) => {}
                    Ok(Err(error)) => {
                        view.control_error = Some(format!("Could not choose folders: {error}"))
                    }
                    Err(error) => {
                        view.control_error = Some(format!("Folder picker closed: {error}"))
                    }
                }
                cx.notify();
            });
        }));
    }

    fn apply_scan(&mut self, mut next: Vec<Listener>, cx: &mut Context<Self>) {
        let old = self
            .listeners
            .iter()
            .filter(|r| !self.removed.contains(&(r.pid, r.port)))
            .map(|r| (r.pid, r.port))
            .collect::<BTreeSet<_>>();
        let new = next
            .iter()
            .map(|r| (r.pid, r.port))
            .collect::<BTreeSet<_>>();
        self.revision += 1;
        self.added = if self.has_scanned {
            new.difference(&old).copied().collect()
        } else {
            BTreeSet::new()
        };
        self.removed = old.difference(&new).copied().collect();
        next.extend(
            self.listeners
                .iter()
                .filter(|r| self.removed.contains(&(r.pid, r.port)))
                .cloned(),
        );
        self.listeners = next;
        self.has_scanned = true;
        if !self.added.is_empty() || !self.removed.is_empty() {
            self.transition_task = Some(cx.spawn(async move |view, cx| {
                cx.background_executor()
                    .timer(theme::TRANSITION_RETENTION)
                    .await;
                let _ = view.update(cx, |view, cx| {
                    view.listeners
                        .retain(|r| !view.removed.contains(&(r.pid, r.port)));
                    view.removed.clear();
                    view.added.clear();
                    cx.notify();
                });
            }));
        }
    }

    fn confirm_stop(&mut self, rows: Vec<Listener>, force: bool, cx: &mut Context<Self>) {
        self.control_error = None;
        self.confirmation = Some((rows, force));
        cx.notify();
    }

    fn perform_stop(&mut self, cx: &mut Context<Self>) {
        let Some((rows, force)) = self.confirmation.take() else {
            return;
        };
        if self.stopping {
            return;
        }
        self.stopping = true;
        let task = cx.background_spawn(async move { ports::stop(&rows, force) });
        self.control_task = Some(cx.spawn(async move |view, cx| {
            let result = task.await;
            let _ = view.update(cx, |view, cx| {
                view.stopping = false;
                if let Err(error) = result {
                    view.control_error = Some(error);
                } else {
                    view.refresh(cx);
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        if self.scanning {
            self.refresh_pending = true;
            return;
        }
        self.scanning = true;
        self.error = None;
        cx.notify();
        let roots = self.project_roots.clone();
        let scan = cx.background_spawn(async move { ports::scan(&roots) });
        self.scan_task = Some(cx.spawn(async move |view, cx| {
            let result = scan.await;
            let _ = view.update(cx, |view, cx| {
                view.scanning = false;
                match result {
                    Ok(listeners) => view.apply_scan(listeners, cx),
                    Err(error) => view.error = Some(error),
                }
                if view.refresh_pending {
                    view.refresh_pending = false;
                    view.refresh(cx);
                }
                cx.notify();
            });
        }));
    }
}

fn has_children(group: &Group, endpoint_count: usize) -> bool {
    endpoint_count > 1 && group.category != Category::Unknown
}

fn cell(text: impl Into<SharedString>, width: f32) -> Div {
    let colors = theme::palette();
    div()
        .w(px(width))
        .flex_shrink_0()
        .truncate()
        .text_color(colors.secondary)
        .child(text.into())
}

impl PortList {
    fn settings_view(&self, cx: &mut Context<Self>) -> Div {
        let colors = theme::palette();
        let metrics = Metrics::at_zoom(self.zoom);
        let mut content = div().id("settings-content").flex_1().min_h_0().overflow_y_scroll()
            .flex().flex_col().p_4().gap_4()
            .child(div().flex().items_center().justify_between()
                .child(div().flex().flex_col().gap_1()
                    .child("Automatic refresh")
                    .child(div().text_size(px(metrics.small)).text_color(colors.secondary)
                        .child(format!("Check every {} seconds while the window is active.", theme::REFRESH_INTERVAL.as_secs()))))
                .child(div().id("auto-refresh").px_3().py_1().rounded_sm().cursor_pointer()
                    .bg(if self.auto_refresh { colors.selected } else { colors.surface })
                    .text_color(if self.auto_refresh { colors.accent } else { colors.secondary })
                    .on_click(cx.listener(|view, _, _, cx| {
                        view.auto_refresh = !view.auto_refresh; view.save_settings();
                        if view.auto_refresh { view.refresh(cx); } cx.notify();
                    })).child(if self.auto_refresh { "On ✓" } else { "Off" })))
            .child(div().flex().items_center().justify_between()
                .child(div().flex().flex_col().gap_1().child("Text size")
                    .child(div().text_size(px(metrics.small)).text_color(colors.secondary).child("Cmd/Ctrl + or − to resize; 0 to reset.")))
                .child(div().text_color(colors.secondary).child(format!("{} px", metrics.text))))
            .child(div().h(px(1.0)).bg(colors.border).flex_shrink_0())
            .child(div().flex().items_center().justify_between().child("Project folders")
                .child(div().id("add-folders").px_2().py_1().rounded_sm().cursor_pointer()
                    .text_color(colors.accent).hover(|s| s.bg(colors.button_hover))
                    .on_click(cx.listener(|view, _, _, cx| view.add_folders(cx))).child("Add folders…")))
            .child(div().text_size(px(metrics.small)).text_color(colors.secondary)
                .child("Listeners running inside these folders appear under Your Projects. Changes save automatically."));
        if self.project_roots.is_empty() {
            content = content.child(
                div()
                    .text_color(colors.muted)
                    .child("No project folders yet."),
            );
        }
        for (index, root) in self.project_roots.iter().enumerate() {
            let path = root.clone();
            content = content.child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .px_3()
                    .py_2()
                    .rounded_sm()
                    .bg(colors.surface)
                    .child(div().flex_1().min_w_0().child(root.display().to_string()))
                    .child(
                        div()
                            .id(("remove-root", index))
                            .px_2()
                            .py_1()
                            .cursor_pointer()
                            .text_color(colors.secondary)
                            .hover(|s| s.text_color(colors.danger))
                            .on_click(cx.listener(move |view, _, _, cx| {
                                view.project_roots.retain(|root| root != &path);
                                view.save_settings();
                                view.refresh(cx);
                                cx.notify();
                            }))
                            .child("Remove"),
                    ),
            );
        }
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_4()
                    .pl(px(metrics.traffic_lights_inset))
                    .pr_3()
                    .h(px(metrics.header))
                    .flex_shrink_0()
                    .border_b_1()
                    .border_color(colors.border)
                    .child(
                        div()
                            .id("back-to-ports")
                            .px_2()
                            .py_1()
                            .rounded_sm()
                            .cursor_pointer()
                            .text_color(colors.accent)
                            .hover(|s| s.bg(colors.button_hover))
                            .on_click(cx.listener(|view, _, _, cx| {
                                view.settings_open = false;
                                cx.notify();
                            }))
                            .child("← Ports"),
                    )
                    .child("Settings"),
            )
            .child(content)
            .child(
                div()
                    .px_4()
                    .py_2()
                    .border_t_1()
                    .border_color(colors.border)
                    .flex_shrink_0()
                    .text_size(px(metrics.small))
                    .text_color(colors.secondary)
                    .child(
                        div()
                            .id("settings-about")
                            .cursor_pointer()
                            .on_click(|_, _, cx| cx.dispatch_action(&crate::About))
                            .child(format!(
                                "About Local Haunt · v{}",
                                env!("CARGO_PKG_VERSION")
                            )),
                    ),
            )
    }

    fn endpoint_row(
        &self,
        group: &Group,
        endpoint: &Endpoint<'_>,
        nested: bool,
        number: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = theme::palette();
        let metrics = Metrics::at_zoom(self.zoom);
        let row = endpoint
            .rows
            .iter()
            .find(|r| !self.removed.contains(&(r.pid, r.port)))
            .copied()
            .unwrap_or(endpoint.rows[0]);
        let port = row.port;
        let row_key = format!(
            "{}:{}:{}:{:?}:{:?}",
            group.key, port, row.process, row.executable, row.directory
        );
        let expanded = self.expanded_rows.contains(&row_key);
        let label = if nested || group.category == Category::Unknown {
            row.process.clone()
        } else {
            group.label.clone()
        };
        let pid_text = if endpoint.rows.len() == 1 {
            row.pid.to_string()
        } else {
            format!("{} PIDs", endpoint.rows.len())
        };
        let key = row_key.clone();
        let targets = endpoint
            .rows
            .iter()
            .filter(|r| !self.removed.contains(&(r.pid, r.port)))
            .map(|r| (*r).clone())
            .collect::<Vec<_>>();
        let force_targets = targets.clone();
        let retiring = endpoint
            .rows
            .iter()
            .all(|r| self.removed.contains(&(r.pid, r.port)));
        let added = endpoint
            .rows
            .iter()
            .any(|r| self.added.contains(&(r.pid, r.port)));
        let animation_key = format!("change-{row_key}-{}", self.revision);
        let header = div()
            .id(SharedString::from(row_key.clone()))
            .flex()
            .items_center()
            .h(px(metrics.row))
            .flex_shrink_0()
            .px_3()
            .gap_2()
            .bg(if number.is_multiple_of(2) {
                colors.row_alternate
            } else {
                colors.row_background
            })
            .hover(|s| s.bg(colors.row_hover))
            // Leaf rows have no hierarchy arrow; click the row or details action for metadata.
            .cursor_pointer()
            .on_click(cx.listener(move |view, _, _, cx| {
                toggle(&mut view.expanded_rows, key.clone());
                cx.notify();
            }))
            .child(div().w(px(metrics.disclosure)).flex_shrink_0())
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .when(nested, |d| d.pl(px(metrics.child_indent)))
                    .truncate()
                    .child(label),
            )
            .child(
                div()
                    .w(px(metrics.port))
                    .flex_shrink_0()
                    .text_color(colors.accent)
                    .child(format!(":{port}")),
            )
            .child(cell(row.process.clone(), metrics.process))
            .child(cell(pid_text, metrics.pid))
            .child(
                div()
                    .w(px(metrics.actions))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(
                        div()
                            .id(SharedString::from(format!("stop-{row_key}")))
                            .px_2()
                            .py_1()
                            .rounded_sm()
                            .text_size(px(metrics.small))
                            .text_color(colors.danger)
                            .hover(|s| s.bg(colors.button_hover))
                            .on_click(cx.listener(move |view, _, _, cx| {
                                cx.stop_propagation();
                                if !retiring && !view.stopping {
                                    view.confirm_stop(targets.clone(), false, cx);
                                }
                            }))
                            .child(if retiring { "Exited" } else { "Stop" }),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!("http-{row_key}")))
                            .px_2()
                            .py_1()
                            .rounded_sm()
                            .text_size(px(metrics.small))
                            .text_color(colors.accent)
                            .hover(|s| s.bg(colors.button_hover))
                            .on_click(move |_, _, cx| {
                                cx.stop_propagation();
                                cx.open_url(&format!("http://localhost:{port}"));
                            })
                            .child("HTTP ↗"),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!("details-{row_key}")))
                            .px_2()
                            .py_1()
                            .rounded_sm()
                            .text_color(if expanded {
                                colors.accent
                            } else {
                                colors.secondary
                            })
                            .hover(|s| s.bg(colors.button_hover))
                            .on_click(cx.listener(move |view, _, _, cx| {
                                cx.stop_propagation();
                                toggle(&mut view.expanded_rows, row_key.clone());
                                cx.notify();
                            }))
                            .child("⋯"),
                    ),
            );
        let mut container = div().flex().flex_col().flex_shrink_0().child(header);
        if expanded {
            let pids = endpoint
                .rows
                .iter()
                .map(|r| r.pid.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            let addresses = endpoint
                .rows
                .iter()
                .flat_map(|r| r.addresses.iter().cloned())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>()
                .join(", ");
            let mut details = div()
                .ml(px(if nested {
                    metrics.nested_details_inset
                } else {
                    metrics.details_inset
                }))
                .mr_3()
                .px_3()
                .py_2()
                .mb_1()
                .bg(colors.surface)
                .flex()
                .flex_col()
                .gap_2()
                .text_size(px(metrics.small));
            let fields = [
                ("Process", Some(format!("{} · PID {pids}", row.process))),
                ("Listening", Some(addresses)),
                (
                    "Directory",
                    row.directory.as_ref().map(|p| p.display().to_string()),
                ),
                (
                    "Executable",
                    row.executable.as_ref().map(|p| p.display().to_string()),
                ),
            ];
            for (label, value) in fields {
                let Some(value) = value else { continue };
                if value.is_empty() {
                    continue;
                }
                let copy_value = value.clone();
                let tooltip_value = value.clone();
                details = details.child(
                    div()
                        .flex()
                        .gap_3()
                        .min_w_0()
                        .child(
                            div()
                                .w(px(74.0 + f32::from(self.zoom) * 4.0))
                                .flex_shrink_0()
                                .text_color(colors.muted)
                                .child(label),
                        )
                        .child(
                            div()
                                .id(SharedString::from(format!(
                                    "detail-{label}-{port}-{}",
                                    row.pid
                                )))
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_color(colors.secondary)
                                .cursor_pointer()
                                .tooltip(move |window, cx| {
                                    gpui_kit::component::tooltip::Tooltip::new(format!(
                                        "{tooltip_value}\nClick to copy"
                                    ))
                                    .build(window, cx)
                                })
                                .on_click(move |_, _, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(
                                        copy_value.clone(),
                                    ))
                                })
                                .child(value),
                        ),
                );
            }
            let mut controls = div()
                .flex()
                .items_center()
                .pt_2()
                .border_t_1()
                .border_color(colors.border)
                .gap_3()
                .child(
                    div()
                        .id(SharedString::from(format!(
                            "copy-details-{port}-{}",
                            row.pid
                        )))
                        .cursor_pointer()
                        .text_color(colors.accent)
                        .on_click(move |_, _, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(format!(
                                "http://localhost:{port}"
                            )));
                        })
                        .child("Copy URL"),
                )
                .child(div().flex_1())
                .child(
                    div()
                        .id(SharedString::from(format!("force-{port}-{}", row.pid)))
                        .cursor_pointer()
                        .text_color(colors.danger)
                        .on_click(cx.listener(move |view, _, _, cx| {
                            if !retiring && !view.stopping {
                                view.confirm_stop(force_targets.clone(), true, cx);
                            }
                        }))
                        .child("Force Kill…"),
                );
            if let Some(app) = ports::owner_app(row) {
                controls = controls.child(
                    div()
                        .id(SharedString::from(format!("owner-{port}-{}", row.pid)))
                        .cursor_pointer()
                        .text_color(colors.accent)
                        .on_click(cx.listener(move |view, _, _, cx| {
                            let app = app.clone();
                            view.owner_task = Some(cx.spawn(async move |view, cx| {
                                let result = cx
                                    .background_executor()
                                    .spawn(async move { ports::open_owner(&app) })
                                    .await;
                                let _ = view.update(cx, |view, cx| {
                                    if let Err(error) = result {
                                        view.control_error = Some(error);
                                    }
                                    cx.notify();
                                });
                            }));
                        }))
                        .child("Open app ↗"),
                );
            }
            container = container.child(details.child(controls));
        }
        if retiring {
            container
                .with_animation(
                    SharedString::from(animation_key),
                    Animation::new(theme::ROW_TRANSITION),
                    |d, progress| d.opacity(1.0 - progress),
                )
                .into_any_element()
        } else if added {
            container
                .with_animation(
                    SharedString::from(animation_key),
                    Animation::new(theme::ROW_TRANSITION),
                    |d, progress| d.opacity(0.4 + 0.6 * progress),
                )
                .into_any_element()
        } else {
            container.into_any_element()
        }
    }
}

fn toggle(set: &mut BTreeSet<String>, key: String) {
    if !set.remove(&key) {
        set.insert(key);
    }
}

impl Render for PortList {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = theme::palette();
        let metrics = Metrics::at_zoom(self.zoom);
        let query = self
            .search
            .as_ref()
            .map(|search| search.read(cx).value().to_string())
            .unwrap_or_default();
        let searching = !query.trim().is_empty();
        let groups = filtered_groups(&self.listeners, &query, self.projects_only);
        let project_count = groups
            .keys()
            .filter(|g| g.category == Category::Projects)
            .count();
        let endpoint_count: usize = groups.values().map(Vec::len).sum();
        let mut list = div()
            .id("port-list")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .py_1();
        if let Some(error) = &self.error {
            list = list.child(div().p_3().text_color(colors.error).child(error.clone()));
        }
        if !self.scanning && (groups.is_empty() || (self.projects_only && project_count == 0)) {
            list = list.child(
                div()
                    .p_3()
                    .text_color(colors.secondary)
                    .child(if searching {
                        "No matching ports. Try another search or clear the field."
                    } else if self.projects_only {
                        "No listeners found in your project folders. Check All for other services."
                    } else {
                        "No listening TCP ports found."
                    }),
            );
        }
        let mut last_category = None;
        let mut row_number: usize = 0;
        for (group, endpoints) in &groups {
            if self.projects_only && group.category != Category::Projects {
                continue;
            }
            if last_category != Some(group.category) {
                let label = match group.category {
                    Category::Projects => "YOUR PROJECTS",
                    Category::Apps => "APPS & SYSTEM",
                    Category::Unknown => "UNIDENTIFIED",
                };
                list = list.child(
                    div()
                        .px_3()
                        .pt_3()
                        .pb_1()
                        .flex_shrink_0()
                        .text_size(px(metrics.caption))
                        .text_color(colors.muted)
                        .child(label),
                );
                last_category = Some(group.category);
            }
            let is_group = has_children(group, endpoints.len());
            if is_group {
                let open = searching || self.expanded_groups.contains(&group.key);
                let key = group.key.clone();
                let unique_ports = endpoints
                    .iter()
                    .map(|e| e.rows[0].port)
                    .collect::<BTreeSet<_>>()
                    .len();
                let pid_count = endpoints
                    .iter()
                    .flat_map(|e| e.rows.iter().map(|r| r.pid))
                    .collect::<BTreeSet<_>>()
                    .len();
                list = list.child(
                    div()
                        .id(SharedString::from(format!("group-{key}")))
                        .flex()
                        .items_center()
                        .h(px(metrics.row))
                        .flex_shrink_0()
                        .px_3()
                        .gap_2()
                        .bg(if row_number.is_multiple_of(2) {
                            colors.row_alternate
                        } else {
                            colors.row_background
                        })
                        .cursor_pointer()
                        .hover(|s| s.bg(colors.row_hover))
                        .on_click(cx.listener(move |view, _, _, cx| {
                            toggle(&mut view.expanded_groups, key.clone());
                            cx.notify();
                        }))
                        .child(
                            div()
                                .w(px(metrics.disclosure))
                                .flex_shrink_0()
                                .text_color(colors.muted)
                                .child(if open { "⌄" } else { "›" }),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .child(group.label.clone()),
                        )
                        .child(cell(format!("{unique_ports} ports"), metrics.port))
                        .child(cell(
                            format!("{} services", endpoints.len()),
                            metrics.process,
                        ))
                        .child(cell(format!("{pid_count} PIDs"), metrics.pid))
                        .child(div().w(px(metrics.actions)).flex_shrink_0()),
                );
                row_number += 1;
                if !open {
                    continue;
                }
            }
            for endpoint in endpoints {
                list = list.child(self.endpoint_row(group, endpoint, is_group, row_number, cx));
                row_number += 1;
            }
        }
        let mut filters = div().flex().gap_1();
        for (label, projects_only) in [("All", false), ("Projects", true)] {
            filters = filters.child(
                div()
                    .id(label)
                    .px_2()
                    .py_1()
                    .rounded_sm()
                    .cursor_pointer()
                    .bg(if self.projects_only == projects_only {
                        colors.selected
                    } else {
                        colors.surface
                    })
                    .text_color(if self.projects_only == projects_only {
                        colors.accent
                    } else {
                        colors.secondary
                    })
                    .hover(|s| s.bg(colors.button_hover))
                    .on_click(cx.listener(move |view, _, _, cx| {
                        view.projects_only = projects_only;
                        cx.notify();
                    }))
                    .child(label),
            );
        }
        let mut root = div()
            .key_context("PortList")
            .track_focus(self.focus.as_ref().expect("view focus"))
            .on_action(cx.listener(|view, _: &ZoomIn, _, cx| {
                view.zoom = (view.zoom + 1).min(6);
                view.save_settings();
                cx.notify();
            }))
            .on_action(cx.listener(|view, _: &ZoomOut, _, cx| {
                view.zoom = (view.zoom - 1).max(-2);
                view.save_settings();
                cx.notify();
            }))
            .on_action(cx.listener(|view, _: &ResetZoom, _, cx| {
                view.zoom = 0;
                view.save_settings();
                cx.notify();
            }))
            .on_action(cx.listener(|view, _: &FocusSearch, window, cx| {
                view.settings_open = false;
                if let Some(search) = &view.search {
                    search.update(cx, |search, cx| search.focus(window, cx));
                }
                cx.notify();
            }))
            .on_action(cx.listener(|view, _: &OpenSettings, _, cx| {
                view.settings_open = true;
                view.confirmation = None;
                cx.notify();
            }))
            .on_action(cx.listener(|view, _: &BackToPorts, _, cx| {
                view.settings_open = false;
                view.confirmation = None;
                cx.notify();
            }))
            .font_family(theme::FONT_FAMILY)
            .flex()
            .flex_col()
            .size_full()
            .bg(colors.background)
            .text_color(colors.text)
            .text_size(px(metrics.text));
        if self.settings_open {
            root = root.child(self.settings_view(cx));
        } else {
            root = root
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .pl(px(metrics.traffic_lights_inset))
                        .pr_3()
                        .py_2()
                        .h(px(metrics.header))
                        .flex_shrink_0()
                        .border_b_1()
                        .border_color(colors.border)
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_3()
                                .child(
                                    img(self.ghost.as_ref().expect("ghost image").clone())
                                        .size(px(metrics.ghost))
                                        .flex_shrink_0(),
                                )
                                .child("Local Haunt"),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(
                                    div()
                                        .id("refresh")
                                        .px_2()
                                        .py_1()
                                        .rounded_sm()
                                        .cursor_pointer()
                                        .text_color(colors.secondary)
                                        .hover(|s| s.bg(colors.button_hover))
                                        .on_click(cx.listener(|view, _, _, cx| view.refresh(cx)))
                                        .flex()
                                        .items_center()
                                        .gap_2()
                                        .child(
                                            div()
                                                .w(px(6.0))
                                                .h(px(6.0))
                                                .flex_shrink_0()
                                                .rounded_full()
                                                .bg(colors.accent)
                                                .opacity(if self.scanning { 1.0 } else { 0.0 }),
                                        )
                                        .child("Refresh"),
                                )
                                .child(
                                    div()
                                        .id("settings")
                                        .px_2()
                                        .py_1()
                                        .rounded_sm()
                                        .cursor_pointer()
                                        .text_color(colors.secondary)
                                        .hover(|s| {
                                            s.bg(colors.button_hover).text_color(colors.text)
                                        })
                                        .on_click(cx.listener(|view, _, _, cx| {
                                            view.settings_open = true;
                                            view.confirmation = None;
                                            cx.notify();
                                        }))
                                        .child("Settings…"),
                                ),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_3()
                        .px_3()
                        .py_2()
                        .flex_shrink_0()
                        .border_b_1()
                        .border_color(colors.border)
                        .child(filters)
                        .child(div().flex_1())
                        .child(
                            div()
                                .w(px(280.0 + f32::from(self.zoom) * 8.0))
                                .min_w_0()
                                .child(
                                    Input::new(self.search.as_ref().expect("search input"))
                                        .id("port-search")
                                        .small()
                                        .cleanable(true)
                                        .bg(colors.surface)
                                        .border_color(colors.border)
                                        .text_size(px(metrics.small)),
                                ),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .px_3()
                        .h(px(metrics.column_header))
                        .flex_shrink_0()
                        .border_b_1()
                        .border_color(colors.border)
                        .text_size(px(metrics.small))
                        .text_color(colors.muted)
                        .child(div().w(px(metrics.disclosure)).flex_shrink_0())
                        .child(div().flex_1().min_w_0().child("Name"))
                        .child(cell("Port", metrics.port))
                        .child(cell("Process", metrics.process))
                        .child(cell("PID", metrics.pid))
                        .child(
                            div()
                                .w(px(metrics.actions))
                                .flex_shrink_0()
                                .child("Actions"),
                        ),
                )
                .child(list)
                .child(
                    div()
                        .px_3()
                        .py_2()
                        .flex()
                        .items_center()
                        .gap_3()
                        .flex_shrink_0()
                        .border_t_1()
                        .border_color(colors.border)
                        .text_size(px(metrics.caption))
                        .text_color(colors.muted)
                        .child(div().flex_1().child(format!(
                            "{project_count} projects · {endpoint_count} endpoints · Auto {}",
                            if self.auto_refresh { "on" } else { "off" }
                        )))
                        .child(
                            div()
                                .id("about-version")
                                .cursor_pointer()
                                .hover(|s| s.text_color(colors.text))
                                .on_click(|_, _, cx| cx.dispatch_action(&crate::About))
                                .child(format!("v{}", env!("CARGO_PKG_VERSION"))),
                        ),
                );
        }
        if let Some(error) = &self.control_error {
            root = root.child(
                div()
                    .px_3()
                    .py_2()
                    .flex()
                    .gap_3()
                    .text_color(colors.error)
                    .child(div().flex_1().child(error.clone()))
                    .child(
                        div()
                            .id("dismiss-error")
                            .cursor_pointer()
                            .on_click(cx.listener(|view, _, _, cx| {
                                view.control_error = None;
                                cx.notify();
                            }))
                            .child("Dismiss"),
                    ),
            );
        }
        if let Some((rows, force)) = &self.confirmation {
            let pids = rows
                .iter()
                .map(|r| r.pid.to_string())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>()
                .join(", ");
            root = root.child(
                div()
                    .px_3()
                    .py_2()
                    .bg(colors.danger_surface)
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(div().flex_1().child(format!(
                        "{} {} · PID {}? {}",
                        if *force { "Force kill" } else { "Stop" },
                        rows[0].process,
                        pids,
                        if *force {
                            "SIGKILL ends it immediately."
                        } else {
                            "SIGTERM lets it shut down."
                        }
                    )))
                    .child(
                        div()
                            .id("confirm-stop")
                            .cursor_pointer()
                            .text_color(colors.danger)
                            .on_click(cx.listener(|view, _, _, cx| view.perform_stop(cx)))
                            .child("Confirm"),
                    )
                    .child(
                        div()
                            .id("cancel-stop")
                            .cursor_pointer()
                            .on_click(cx.listener(|view, _, _, cx| {
                                view.confirmation = None;
                                cx.notify();
                            }))
                            .child("Cancel"),
                    ),
            );
        }
        root
    }
}

#[cfg(test)]
mod tests {
    use super::{BackToPorts, OpenSettings, PortList, filtered_groups, grouped, has_children};
    use crate::ports::{Category, Group, Listener};
    use gpui_kit::{AppContext, TestAppContext};
    #[gpui_kit::test]
    fn settings_navigation_preserves_the_port_view(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        cx.update(crate::show_ports);
        cx.run_until_parked();
        let handle = cx.update(|cx| cx.windows()[0]);
        let window = handle.downcast::<PortList>().unwrap();
        window
            .update(cx, |view, window, cx| {
                view.search
                    .as_ref()
                    .unwrap()
                    .update(cx, |input, cx| input.set_value("3000", window, cx));
                view.expanded_rows.insert("retained".into());
            })
            .unwrap();
        cx.dispatch_action(handle, OpenSettings);
        window
            .update(cx, |view, _, _| assert!(view.settings_open))
            .unwrap();
        cx.dispatch_action(handle, BackToPorts);
        window
            .update(cx, |view, _, cx| {
                assert_eq!(view.search.as_ref().unwrap().read(cx).value(), "3000");
                assert!(!view.settings_open);
                assert!(view.expanded_rows.contains("retained"));
            })
            .unwrap();
    }

    #[gpui_kit::test]
    fn exited_rows_are_retained_briefly_then_removed(cx: &mut TestAppContext) {
        let entity = cx.new(|_| PortList::default());
        let row = Listener {
            port: 1234,
            pid: 10,
            identity: None,
            process: "node".into(),
            addresses: vec![],
            directory: None,
            project: None,
            executable: None,
            group: Group::default(),
        };
        entity.update(cx, |view, cx| {
            view.apply_scan(vec![row.clone()], cx);
            assert!(
                view.added.is_empty(),
                "Initial scan does not animate every listener"
            );
            view.expanded_groups.insert("site".into());
            view.apply_scan(vec![], cx);
            assert_eq!(view.listeners.len(), 1);
            assert!(view.removed.contains(&(10, 1234)));
            assert!(view.expanded_groups.contains("site"));
        });
        cx.run_until_parked();
        cx.executor()
            .advance_clock(std::time::Duration::from_secs(1));
        cx.run_until_parked();
        entity.update(cx, |view, cx| {
            assert!(view.listeners.is_empty());
            view.apply_scan(vec![row], cx);
            assert!(view.added.contains(&(10, 1234)));
        });
    }

    #[test]
    fn search_keeps_endpoint_workers_and_respects_project_filter() {
        let row = Listener {
            port: 3000,
            pid: 42,
            identity: None,
            process: "node".into(),
            addresses: vec![],
            directory: None,
            project: Some("My Site".into()),
            executable: None,
            group: Group {
                category: Category::Projects,
                key: "/site".into(),
                label: "My Site".into(),
            },
        };
        let mut worker = row.clone();
        worker.pid = 43;
        let mut other = row.clone();
        other.port = 8080;
        other.pid = 99;
        other.process = "python".into();
        other.project = None;
        other.group = Group::default();
        let rows = [row, worker, other];
        for query in ["42", " :3000 ", "NODE", "my site"] {
            let groups = filtered_groups(&rows, query, false);
            assert_eq!(groups.len(), 1);
            let endpoints = groups.values().next().unwrap();
            assert_eq!(endpoints.len(), 1);
            assert_eq!(
                endpoints[0].rows.len(),
                2,
                "A PID search must retain all endpoint workers"
            );
        }
        assert_eq!(filtered_groups(&rows, "  ", false).len(), 2);
        assert_eq!(filtered_groups(&rows, "", true).len(), 1);
        assert!(filtered_groups(&rows, "python", true).is_empty());
        assert!(filtered_groups(&rows, "missing", false).is_empty());
    }

    #[test]
    fn only_related_multiple_endpoints_get_a_parent_row() {
        let project = Group {
            category: Category::Projects,
            key: "site".into(),
            label: "site".into(),
        };
        assert!(!has_children(&project, 1));
        assert!(has_children(&project, 2));
        assert!(!has_children(&Group::default(), 2));
    }

    #[test]
    fn folds_workers_without_merging_different_services() {
        let row = Listener {
            port: 80,
            pid: 10,
            identity: None,
            process: "nginx".into(),
            addresses: vec!["*:80".into()],
            directory: None,
            project: None,
            executable: None,
            group: Group::default(),
        };
        let mut worker = row.clone();
        worker.pid = 11;
        let mut different = row.clone();
        different.process = "node".into();
        different.pid = 12;
        let rows = [row, worker, different];
        let groups = grouped(&rows);
        let endpoints = groups.values().next().unwrap();
        assert_eq!(endpoints.len(), 2);
        assert_eq!(endpoints[0].rows.len(), 2);
    }
}
