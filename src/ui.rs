use crate::project::Project;
use crate::splash::Splash;
use crate::theme::Theme;
use anyhow::Result;
use iocraft::prelude::*;
use nucleo::{
    pattern::{CaseMatching, Normalization, Pattern},
    Config, Matcher, Utf32Str,
};
use std::os::unix::io::AsRawFd;
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    Cd,
    Copy,
    CodeClaude,
    CodeCodex,
    CodeOpencode,
    Finder,
    Launch,
}

impl Action {
    fn label(&self) -> &'static str {
        match self {
            Action::Cd => "cd",
            Action::Copy => "copy",
            Action::CodeClaude => "claude",
            Action::CodeCodex => "codex",
            Action::CodeOpencode => "opencode",
            Action::Finder => "finder",
            Action::Launch => "launch",
        }
    }

    fn is_code_subaction(&self) -> bool {
        matches!(
            self,
            Action::CodeClaude | Action::CodeCodex | Action::CodeOpencode
        )
    }
}

const ACTIONS: [Action; 7] = [
    Action::Cd,
    Action::Copy,
    Action::CodeClaude,
    Action::CodeCodex,
    Action::CodeOpencode,
    Action::Finder,
    Action::Launch,
];

fn filter_projects(projects: &[Project], query: &str) -> Vec<(usize, u32)> {
    if query.is_empty() {
        return projects.iter().enumerate().map(|(i, _)| (i, 0)).collect();
    }
    let mut buf = Vec::new();
    let pattern = Pattern::parse(query, CaseMatching::Smart, Normalization::Smart);
    let mut matcher = Matcher::new(Config::DEFAULT);
    let mut results: Vec<_> = projects
        .iter()
        .enumerate()
        .filter_map(|(i, p)| {
            let haystack = format!(
                "{} {} {}",
                p.name,
                p.relative_path,
                p.description.as_deref().unwrap_or("")
            );
            pattern
                .score(Utf32Str::new(&haystack, &mut buf), &mut matcher)
                .map(|score| (i, score))
        })
        .collect();
    results.sort_by(|a, b| b.1.cmp(&a.1));
    results
}

fn scroll_offset(selected: usize, visible: usize, total: usize) -> usize {
    if total <= visible || selected < visible / 2 {
        return 0;
    }
    if selected + visible / 2 >= total {
        total - visible
    } else {
        selected - visible / 2
    }
}

struct ListRow {
    is_selected: bool,
    name: String,
    framework: Option<String>,
    project_type: Option<String>,
}

struct PreviewLine {
    text: String,
    color: Color,
}

#[derive(Default, Props)]
struct PickerProps<'a> {
    projects: Vec<Project>,
    theme: Option<Theme>,
    cache_age: Option<String>,
    result_out: Option<&'a mut Option<Project>>,
}

#[component]
fn Picker<'a>(props: &mut PickerProps<'a>, mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let mut system = hooks.use_context_mut::<SystemContext>();
    let (term_w, term_h) = hooks.use_terminal_size();
    let mut query = hooks.use_state(String::new);
    let mut selected = hooks.use_state(|| 0usize);
    let mut exit_action = hooks.use_state(|| 0u8); // 0=none, 1=quit, 2=select
    let mut selected_action = hooks.use_state(|| 0usize); // Index into ACTIONS
    let mut in_code_submenu = hooks.use_state(|| false);
    let mut do_copy = hooks.use_state(|| false); // Flag to trigger copy operation

    // Theme state for live cycling
    let mut current_theme = hooks.use_state({
        let initial = props.theme.unwrap_or_else(Theme::dark);
        move || initial
    });
    let t = current_theme.get();

    // Theme toast state
    let mut show_theme_toast = hooks.use_state(|| false);
    let mut theme_toast_name = hooks.use_state(String::new);
    let dismiss_toast = hooks.use_async_handler(move |()| async move {
        smol::Timer::after(Duration::from_millis(1500)).await;
        show_theme_toast.set(false);
    });

    // Copy toast state
    let mut show_copy_toast = hooks.use_state(|| false);
    let mut copy_toast_msg = hooks.use_state(String::new);
    let dismiss_copy_toast = hooks.use_async_handler(move |()| async move {
        smol::Timer::after(Duration::from_millis(1500)).await;
        show_copy_toast.set(false);
    });

    // Splash state — visible for 2s then gone
    let mut show_splash = hooks.use_state(|| true);
    hooks.use_future(async move {
        smol::Timer::after(Duration::from_secs(2)).await;
        show_splash.set(false);
    });

    // Cache age string (computed once from props)
    let cache_age = hooks.use_state({
        let age = props.cache_age.take().unwrap_or_default();
        move || age
    });

    // Projects state: owned by component, refreshable via Ctrl+R
    let mut projects_state =
        hooks.use_state::<Vec<Project>, _>(|| std::mem::take(&mut props.projects));
    let mut refresh_status = hooks.use_state(|| 0u8); // 0=idle, 1=loading, 2=success, 3=error
    let mut refresh_msg = hooks.use_state(String::new);

    let refresh = hooks.use_async_handler(move |()| async move {
        refresh_status.set(1);
        match smol::unblock(crate::project::refresh_projects).await {
            Ok(new_projects) => {
                let count = new_projects.len();
                projects_state.set(new_projects);
                selected.set(0);
                refresh_msg.set(format!("Refreshed: {count} projects"));
                refresh_status.set(2);
                smol::Timer::after(Duration::from_secs(2)).await;
                if refresh_status.get() == 2 {
                    refresh_status.set(0);
                }
            }
            Err(e) => {
                refresh_msg.set(format!("{e}"));
                refresh_status.set(3);
                smol::Timer::after(Duration::from_secs(3)).await;
                if refresh_status.get() == 3 {
                    refresh_status.set(0);
                }
            }
        }
    });

    let all_projects = projects_state.read();
    let query_str = query.to_string();
    let query_empty = query_str.is_empty();
    let filtered = filter_projects(&all_projects, &query_str);
    let count = filtered.len();
    let sel = if count > 0 {
        selected.get().min(count - 1)
    } else {
        0
    };

    hooks.use_terminal_events({
        let refresh = refresh.clone();
        let dismiss_toast = dismiss_toast.clone();
        move |event| {
            if let TerminalEvent::Key(KeyEvent {
                code,
                kind,
                modifiers,
                ..
            }) = event
            {
                if kind == KeyEventKind::Release {
                    return;
                }
                match code {
                    KeyCode::Char('r') if modifiers.contains(KeyModifiers::CONTROL) => {
                        if refresh_status.get() != 1 {
                            refresh(());
                        }
                    }
                    KeyCode::Char('t') if modifiers.contains(KeyModifiers::CONTROL) => {
                        let cur = current_theme.get();
                        let next = crate::theme::next_theme(cur.name);
                        theme_toast_name.set(next.name.to_string());
                        current_theme.set(next);
                        show_theme_toast.set(true);
                        dismiss_toast(());
                    }
                    KeyCode::Esc => {
                        if in_code_submenu.get() {
                            in_code_submenu.set(false);
                        } else {
                            exit_action.set(1);
                        }
                    }
                    KeyCode::Enter => {
                        // Check if Copy action is selected
                        if count > 0 && !in_code_submenu.get() {
                            let action_idx = selected_action.get();
                            if ACTIONS[action_idx] == Action::Copy {
                                do_copy.set(true);
                                return; // Don't set exit_action
                            }
                        }
                        exit_action.set(2)
                    }
                    KeyCode::Up if count > 0 => {
                        let cur = selected.get() as i32;
                        selected.set((cur - 1).rem_euclid(count as i32) as usize);
                    }
                    KeyCode::Down if count > 0 => {
                        selected.set((selected.get() + 1) % count);
                    }
                    KeyCode::Right => {
                        let cur = selected_action.get();
                        if cur + 1 < ACTIONS.len() {
                            selected_action.set(cur + 1);
                        }
                    }
                    KeyCode::Left => {
                        let cur = selected_action.get();
                        if cur > 0 {
                            selected_action.set(cur - 1);
                        }
                    }
                    KeyCode::Backspace => {
                        let mut q = query.to_string();
                        q.pop();
                        query.set(q);
                        selected.set(0);
                    }
                    KeyCode::Char(c) => {
                        let mut q = query.to_string();
                        q.push(c);
                        query.set(q);
                        selected.set(0);
                    }
                    _ => {}
                }
            }
        }
    });

    // Handle copy action
    if do_copy.get() {
        do_copy.set(false);
        if let Some(&(idx, _)) = filtered.get(sel) {
            let project = &all_projects[idx];
            use arboard::Clipboard;
            match Clipboard::new() {
                Ok(mut clipboard) => match clipboard.set_text(&project.path) {
                    Ok(_) => {
                        copy_toast_msg.set(format!("Copied: {}", project.name));
                        show_copy_toast.set(true);
                        dismiss_copy_toast(());
                    }
                    Err(_) => {
                        copy_toast_msg.set("Failed to copy".to_string());
                        show_copy_toast.set(true);
                        dismiss_copy_toast(());
                    }
                },
                Err(_) => {
                    copy_toast_msg.set("Clipboard unavailable".to_string());
                    show_copy_toast.set(true);
                    dismiss_copy_toast(());
                }
            }
        }
    }

    if exit_action.get() == 1 {
        system.exit();
        return element!(View);
    }
    if exit_action.get() == 2 {
        if let Some(&(idx, _)) = filtered.get(sel) {
            let project = &all_projects[idx];
            let action_idx = selected_action.get();
            let action = ACTIONS[action_idx];

            if in_code_submenu.get() {
                if let Some(out) = props.result_out.as_mut() {
                    let mut p = project.clone();
                    p.scripts = Some(
                        [
                            ("_action".to_string(), format!("{:?}", action)),
                            ("_subaction".to_string(), "code".to_string()),
                        ]
                        .iter()
                        .cloned()
                        .collect(),
                    );
                    **out = Some(p);
                }
            } else if action.is_code_subaction() {
                in_code_submenu.set(true);
                return element!(View);
            } else if let Some(out) = props.result_out.as_mut() {
                let mut p = project.clone();
                p.scripts = Some(
                    [("_action".to_string(), format!("{:?}", action))]
                        .iter()
                        .cloned()
                        .collect(),
                );
                **out = Some(p);
            }
        }
        system.exit();
        return element!(View);
    }

    // Visible list rows
    let splash_visible = show_splash.get();
    let splash_reserve = if splash_visible { 8 } else { 0 };
    let list_h = (term_h as usize)
        .saturating_sub(11)
        .saturating_sub(splash_reserve)
        .max(3);
    let scroll = scroll_offset(sel, list_h, count);
    let rows: Vec<ListRow> = filtered[scroll..(scroll + list_h).min(count)]
        .iter()
        .enumerate()
        .map(|(i, &(idx, _))| {
            let p = &all_projects[idx];
            ListRow {
                is_selected: scroll + i == sel,
                name: p.name.clone(),
                framework: p.framework.clone(),
                project_type: p.project_type.clone(),
            }
        })
        .collect();

    // Column widths for aligned layout
    let max_name = rows.iter().map(|r| r.name.len()).max().unwrap_or(0);
    let name_width = max_name.min((term_w as usize) * 2 / 5);
    let max_fw = rows
        .iter()
        .filter_map(|r| r.framework.as_ref())
        .map(|fw| fw.len() + 3) // " [fw]"
        .max()
        .unwrap_or(0);
    let fw_width = max_fw.min(14);

    // Calculate action buttons area for selected row
    let current_action = ACTIONS[selected_action.get()];
    let in_submenu = in_code_submenu.get();

    // Preview lines
    let preview_lines: Vec<PreviewLine> = if let Some(&(idx, _)) = filtered.get(sel) {
        let p = &all_projects[idx];
        let mut lines = vec![PreviewLine {
            text: format!("Path: {}", p.path),
            color: t.text,
        }];
        if let Some(d) = &p.description {
            lines.push(PreviewLine {
                text: format!("Desc: {d}"),
                color: t.text,
            });
        }
        if let Some(b) = &p.git_branch {
            let color = match p.git.as_deref() {
                Some("clean") => t.success,
                Some("dirty") => t.warning,
                _ => t.text_muted,
            };
            lines.push(PreviewLine {
                text: format!("Git:  {b}"),
                color,
            });
        }
        if let Some(cmd) = &p.dev_command {
            lines.push(PreviewLine {
                text: format!("Dev:  {} run {cmd}", p.runner.as_deref().unwrap_or("npm")),
                color: t.text,
            });
        }
        lines
    } else {
        vec![PreviewLine {
            text: "No project selected".into(),
            color: t.text_muted,
        }]
    };

    // Status text for search bar
    let rs = refresh_status.get();
    let toast_active = show_theme_toast.get();
    let copy_toast_active = show_copy_toast.get();
    let status_text = if copy_toast_active {
        copy_toast_msg.to_string()
    } else if toast_active {
        format!("Theme: {}", *theme_toast_name.read())
    } else {
        match rs {
            1 => "Refreshing...".to_string(),
            2 => refresh_msg.to_string(),
            3 => format!("Error: {}", *refresh_msg.read()),
            _ => {
                let age_str = cache_age.to_string();
                if age_str.is_empty() {
                    format!("{count} projects")
                } else {
                    format!("{count} projects \u{b7} {age_str}")
                }
            }
        }
    };
    let status_color = if copy_toast_active {
        t.success
    } else if toast_active {
        t.accent
    } else {
        match rs {
            1 => t.warning,
            2 => t.success,
            3 => t.error,
            _ => t.text_muted,
        }
    };

    element! {
        View(width: term_w, height: term_h, flex_direction: FlexDirection::Column, background_color: t.bg) {
            // Search
            View(
                height: 3u32,
                border_style: BorderStyle::Round,
                border_color: t.border,
                flex_direction: FlexDirection::Row,
                justify_content: JustifyContent::SpaceBetween,
            ) {
                Text(
                    content: if query_empty {
                        "Type to search...".to_string()
                    } else {
                        query_str
                    },
                    color: if query_empty { t.text_muted } else { t.accent },
                )
                Text(content: status_text, color: status_color)
            }
            // Project list
            View(
                flex_grow: 1.0,
                flex_direction: FlexDirection::Column,
                overflow: Overflow::Hidden,
            ) {
                #(rows.iter().map(|row| {
                    let actions_to_show: Vec<Action> = if in_submenu {
                        vec![Action::CodeClaude, Action::CodeCodex, Action::CodeOpencode]
                    } else {
                        ACTIONS.to_vec()
                    };
                    element! {
                        View(
                            flex_direction: FlexDirection::Row,
                            background_color: if row.is_selected { t.selected_bg } else { t.bg },
                        ) {
                            Text(
                                content: " ",
                                color: t.accent,
                            )
                            Text(
                                content: format!("{:<width$}", row.name, width = name_width),
                                color: t.project,
                                weight: Weight::Bold,
                            )
                            Text(
                                content: if let Some(fw) = &row.framework {
                                    format!(" {:<width$}", format!("[{fw}]"), width = fw_width.saturating_sub(1))
                                } else {
                                    format!(" {:<width$}", "", width = fw_width.saturating_sub(1))
                                },
                                color: if row.framework.is_some() { t.framework } else { t.bg },
                            )
                            #(row.project_type.as_ref().map(|pt| {
                                element! {
                                    Text(content: format!(" {pt}"), color: t.text_muted)
                                }
                            }))
                            #(if row.is_selected {
                                let actions_vec = actions_to_show.clone();
                                Some(element! {
                                    View(flex_direction: FlexDirection::Row, flex_grow: 1.0, justify_content: JustifyContent::FlexEnd) {
                                        #(actions_vec.iter().enumerate().map(|(i, action)| {
                                            let is_selected = if in_submenu {
                                                *action == current_action
                                            } else {
                                                selected_action.get() == i
                                            };
                                            let btn_text = format!("[{}]", action.label());
                                            element! {
                                                View {
                                                    Text(
                                                        content: btn_text,
                                                        color: if is_selected { t.action_selected } else { t.action },
                                                        weight: if is_selected { Weight::Bold } else { Weight::Normal },
                                                    )
                                                    Text(content: " ", color: t.action)
                                                }
                                            }
                                        }))
                                    }
                                })
                            } else {
                                None
                            })
                        }
                    }
                }))
                Splash(visible: splash_visible, color: t.text_muted, accent: Some(t.accent))
            }
            // Bottom panes
            View(height: 8u32, flex_direction: FlexDirection::Row) {
                // Preview
                View(
                    width: 50pct,
                    border_style: BorderStyle::Round,
                    border_color: t.border,
                    flex_direction: FlexDirection::Column,
                ) {
                    #(preview_lines.iter().map(|line| {
                        element! {
                            Text(content: line.text.clone(), color: line.color)
                        }
                    }))
                }
                // Hotkeys
                View(
                    width: 50pct,
                    border_style: BorderStyle::Round,
                    border_color: t.border,
                    flex_direction: FlexDirection::Column,
                ) {
                    View(flex_direction: FlexDirection::Row) {
                        Text(content: "\u{2191}/\u{2193}  ", color: t.accent)
                        Text(content: "Navigate", color: t.text)
                    }
                    View(flex_direction: FlexDirection::Row) {
                        Text(content: "Enter ", color: t.accent)
                        Text(content: "Select project", color: t.text)
                    }
                    View(flex_direction: FlexDirection::Row) {
                        Text(content: "Esc   ", color: t.accent)
                        Text(content: "Quit", color: t.text)
                    }
                    View(flex_direction: FlexDirection::Row) {
                        Text(content: "^R    ", color: t.accent)
                        Text(content: "Refresh index", color: t.text)
                    }
                    View(flex_direction: FlexDirection::Row) {
                        Text(content: "^T    ", color: t.accent)
                        Text(content: "Cycle theme", color: t.text)
                    }
                }
            }
        }
    }
}

pub fn run_picker(
    projects: &[Project],
    theme: Theme,
    scanned_at: Option<&str>,
) -> Result<Option<Project>> {
    let mut result: Option<Project> = None;
    let cache_age = scanned_at
        .map(crate::project::format_cache_age)
        .unwrap_or_default();

    // Redirect stdout to /dev/tty so iocraft renders to terminal,
    // keeping original stdout free for shell integration output
    let tty = std::fs::File::options()
        .read(true)
        .write(true)
        .open("/dev/tty")?;
    let saved_stdout = unsafe { libc::dup(libc::STDOUT_FILENO) };
    anyhow::ensure!(saved_stdout != -1, "failed to dup stdout");
    unsafe { libc::dup2(tty.as_raw_fd(), libc::STDOUT_FILENO) };
    drop(tty);

    let render_result = smol::block_on(
        element! {
            Picker(
                projects: projects.to_vec(),
                theme: Some(theme),
                cache_age: Some(cache_age),
                result_out: &mut result,
            )
        }
        .fullscreen(),
    );

    // Restore original stdout
    unsafe {
        libc::dup2(saved_stdout, libc::STDOUT_FILENO);
        libc::close(saved_stdout);
    }

    render_result?;
    Ok(result)
}
