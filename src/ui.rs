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
use std::path::PathBuf;
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct Selection {
    pub project: Project,
    pub action: SelectionAction,
}

#[derive(Clone, Debug)]
pub enum SelectionAction {
    Cd,
    Open(OpenAction),
    Code(CodeAction),
    Run(RunAction),
    Agent(AgentAction),
    Copy(CopyAction),
    Deploy(DeploySelection),
}

#[derive(Clone, Debug)]
pub struct DeploySelection {
    pub platform: String,
    pub url: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenAction {
    Iterm,
    Finder,
    Open,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CodeAction {
    Claude,
    Codex,
    Opencode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunAction {
    Dev,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentAction {
    Claude,
    Agents,
    CopyClaudeToAgents,
    CopyAgentsToClaude,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CopyAction {
    Path,
    DevCommand,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RootAction {
    Cd,
    Open,
    Code,
    Run,
    Agent,
    Copy,
    Deploy,
}

impl RootAction {
    fn label(&self) -> &'static str {
        match self {
            RootAction::Cd => "cd",
            RootAction::Open => "open",
            RootAction::Code => "code",
            RootAction::Run => "run",
            RootAction::Agent => "agent",
            RootAction::Copy => "copy",
            RootAction::Deploy => "deploy",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SubmenuKind {
    Open,
    Code,
    Run,
    Agent,
    Copy,
    Deploy,
}

#[derive(Clone)]
struct ActionItem {
    label: String,
    selected: bool,
}

#[derive(Clone)]
struct Tag {
    text: String,
    color: Color,
}

const COLOR_BLUE: Color = Color::Rgb {
    r: 59,
    g: 130,
    b: 246,
};
const COLOR_GREEN: Color = Color::Rgb {
    r: 34,
    g: 197,
    b: 94,
};
const COLOR_ORANGE: Color = Color::Rgb {
    r: 249,
    g: 115,
    b: 22,
};
const COLOR_PURPLE: Color = Color::Rgb {
    r: 168,
    g: 85,
    b: 247,
};
const COLOR_RED: Color = Color::Rgb {
    r: 239,
    g: 68,
    b: 68,
};
const COLOR_YELLOW: Color = Color::Rgb {
    r: 234,
    g: 179,
    b: 8,
};
const COLOR_GRAY: Color = Color::Rgb {
    r: 156,
    g: 163,
    b: 175,
};

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
    tags: Vec<Tag>,
    action_items: Vec<ActionItem>,
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
    base_dir: PathBuf,
    api_url: String,
    result_out: Option<&'a mut Option<Selection>>,
}

#[component]
fn Picker<'a>(props: &mut PickerProps<'a>, mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let mut system = hooks.use_context_mut::<SystemContext>();
    let (term_w, term_h) = hooks.use_terminal_size();
    let mut query = hooks.use_state(String::new);
    let mut selected = hooks.use_state(|| 0usize);
    let mut exit_action = hooks.use_state(|| 0u8); // 0=none, 1=quit, 2=select

    let mut root_action_index = hooks.use_state(|| 0usize);
    let mut sub_action_index = hooks.use_state(|| 0usize);
    let mut submenu = hooks.use_state(|| None::<SubmenuKind>);

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

    // Splash state — visible for 2s then gone
    let mut show_splash = hooks.use_state(|| true);
    hooks.use_future(async move {
        smol::Timer::after(Duration::from_secs(2)).await;
        show_splash.set(false);
    });

    // Cache age string (computed once from props)
    let mut cache_age = hooks.use_state({
        let age = props.cache_age.take().unwrap_or_default();
        move || age
    });

    // Projects state: owned by component, refreshable via Ctrl+R
    let mut projects_state =
        hooks.use_state::<Vec<Project>, _>(|| std::mem::take(&mut props.projects));
    let mut refresh_status = hooks.use_state(|| 0u8); // 0=idle, 1=loading, 2=success, 3=error
    let mut refresh_msg = hooks.use_state(String::new);

    let base_dir = props.base_dir.clone();
    let api_url = props.api_url.clone();
    let refresh = hooks.use_async_handler(move |()| {
        let base_dir = base_dir.clone();
        let api_url = api_url.clone();
        async move {
            refresh_status.set(1);
            match smol::unblock(move || crate::project::fetch_projects(&api_url, &base_dir)).await {
                Ok(result) => {
                    let count = result.projects.len();
                    projects_state.set(result.projects);
                    selected.set(0);
                    cache_age.set(
                        result
                            .scanned_at
                            .as_deref()
                            .map(crate::project::format_cache_age)
                            .unwrap_or_default(),
                    );
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
                        if submenu.get().is_some() {
                            submenu.set(None);
                        } else {
                            exit_action.set(1);
                        }
                    }
                    KeyCode::Enter => exit_action.set(2),
                    KeyCode::Up if count > 0 => {
                        let cur = selected.get() as i32;
                        selected.set((cur - 1).rem_euclid(count as i32) as usize);
                        submenu.set(None);
                    }
                    KeyCode::Down if count > 0 => {
                        selected.set((selected.get() + 1) % count);
                        submenu.set(None);
                    }
                    KeyCode::Right => {
                        if submenu.get().is_some() {
                            let cur = sub_action_index.get();
                            sub_action_index.set(cur.saturating_add(1));
                        } else {
                            let cur = root_action_index.get();
                            root_action_index.set(cur.saturating_add(1));
                        }
                    }
                    KeyCode::Left => {
                        if submenu.get().is_some() {
                            let cur = sub_action_index.get();
                            if cur > 0 {
                                sub_action_index.set(cur - 1);
                            }
                        } else {
                            let cur = root_action_index.get();
                            if cur > 0 {
                                root_action_index.set(cur - 1);
                            }
                        }
                    }
                    KeyCode::Backspace => {
                        let mut q = query.to_string();
                        q.pop();
                        query.set(q);
                        selected.set(0);
                        submenu.set(None);
                    }
                    KeyCode::Char(c) => {
                        let mut q = query.to_string();
                        q.push(c);
                        query.set(q);
                        selected.set(0);
                        submenu.set(None);
                    }
                    _ => {}
                }
            }
        }
    });

    if exit_action.get() == 1 {
        system.exit();
        return element!(View);
    }

    let current_project = filtered.get(sel).map(|(idx, _)| &all_projects[*idx]);
    let root_actions = build_root_actions(current_project);
    if root_actions.is_empty() {
        root_action_index.set(0);
    } else if root_action_index.get() >= root_actions.len() {
        root_action_index.set(root_actions.len() - 1);
    }

    let current_root_action = root_actions.get(root_action_index.get()).copied();

    let subactions = match (submenu.get(), current_project) {
        (Some(kind), Some(project)) => build_subactions(kind, project),
        _ => Vec::new(),
    };
    if sub_action_index.get() >= subactions.len() && !subactions.is_empty() {
        sub_action_index.set(subactions.len() - 1);
    }

    if exit_action.get() == 2 {
        if let Some(project) = current_project {
            if submenu.get().is_some() {
                if let Some(item) = subactions.get(sub_action_index.get()) {
                    if let Some(out) = props.result_out.as_mut() {
                        **out = Some(Selection {
                            project: project.clone(),
                            action: item.clone(),
                        });
                    }
                } else {
                    submenu.set(None);
                    exit_action.set(0);
                }
            } else if let Some(root_action) = current_root_action {
                match root_action {
                    RootAction::Cd => {
                        if let Some(out) = props.result_out.as_mut() {
                            **out = Some(Selection {
                                project: project.clone(),
                                action: SelectionAction::Cd,
                            });
                        }
                    }
                    RootAction::Open => {
                        submenu.set(Some(SubmenuKind::Open));
                        sub_action_index.set(0);
                        exit_action.set(0);
                    }
                    RootAction::Code => {
                        submenu.set(Some(SubmenuKind::Code));
                        sub_action_index.set(0);
                        exit_action.set(0);
                    }
                    RootAction::Run => {
                        submenu.set(Some(SubmenuKind::Run));
                        sub_action_index.set(0);
                        exit_action.set(0);
                    }
                    RootAction::Agent => {
                        submenu.set(Some(SubmenuKind::Agent));
                        sub_action_index.set(0);
                        exit_action.set(0);
                    }
                    RootAction::Copy => {
                        submenu.set(Some(SubmenuKind::Copy));
                        sub_action_index.set(0);
                        exit_action.set(0);
                    }
                    RootAction::Deploy => {
                        submenu.set(Some(SubmenuKind::Deploy));
                        sub_action_index.set(0);
                        exit_action.set(0);
                    }
                }
            }
        }

        if exit_action.get() == 2 {
            system.exit();
            return element!(View);
        }
    }

    // Layout dimensions
    let splash_visible = show_splash.get();
    let splash_reserve = if splash_visible { 8 } else { 0 };
    let total_w = term_w as usize;
    let (name_width, tag_width, action_width) = compute_column_widths(total_w);
    let bottom_h: u32 = if (term_h as usize) < 16 {
        0
    } else if (term_h as usize) < 24 {
        5
    } else {
        8
    };
    let list_h = (term_h as usize)
        .saturating_sub(3 + bottom_h as usize)
        .saturating_sub(splash_reserve)
        .max(1);
    let scroll = scroll_offset(sel, list_h, count);
    let rows: Vec<ListRow> = filtered[scroll..(scroll + list_h).min(count)]
        .iter()
        .enumerate()
        .map(|(i, &(idx, _))| {
            let p = &all_projects[idx];
            let is_selected = scroll + i == sel;
            let tags = build_tags(p, t);

            let action_items = if is_selected {
                fit_action_items(
                    build_action_items(
                        submenu.get(),
                        current_root_action,
                        &root_actions,
                        &subactions,
                        root_action_index.get(),
                        sub_action_index.get(),
                    ),
                    action_width,
                )
            } else {
                Vec::new()
            };

            ListRow {
                is_selected,
                name: p.name.clone(),
                tags,
                action_items,
            }
        })
        .collect();

    // Preview lines
    let preview_lines: Vec<PreviewLine> = if let Some(project) = current_project {
        let mut lines = vec![PreviewLine {
            text: format!("Path: {}", project.path),
            color: t.text,
        }];
        if let Some(d) = &project.description {
            lines.push(PreviewLine {
                text: format!("Desc: {d}"),
                color: t.text,
            });
        }
        if let Some(b) = &project.git_branch {
            let color = match project.git.as_deref() {
                Some("clean") => t.success,
                Some("dirty") => t.warning,
                _ => t.text_muted,
            };
            lines.push(PreviewLine {
                text: format!("Git:  {b}"),
                color,
            });
        }
        if let Some(cmd) = &project.dev_command {
            lines.push(PreviewLine {
                text: format!(
                    "Dev:  {} run {cmd}",
                    project.runner.as_deref().unwrap_or("npm")
                ),
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
    let status_text = if toast_active {
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
    let status_color = if toast_active {
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
                    let visible_tags = fit_tags(&row.tags, tag_width);
                    let name_text = fit_text(&row.name, name_width);
                    let name_cell = format!("{:<width$}", name_text, width = name_width);
                    element! {
                        View(
                            flex_direction: FlexDirection::Row,
                            background_color: if row.is_selected { t.selected_bg } else { t.bg },
                        ) {
                            Text(
                                content: " ",
                                color: t.accent,
                            )
                            View(width: name_width as u32, flex_direction: FlexDirection::Row) {
                                Text(
                                    content: name_cell,
                                    color: t.project,
                                    weight: Weight::Bold,
                                )
                            }
                            Text(content: " ", color: t.text)
                            View(width: tag_width as u32, flex_direction: FlexDirection::Row) {
                                #(visible_tags.iter().enumerate().map(|(i, tag)| {
                                    element! {
                                        View(flex_direction: FlexDirection::Row) {
                                            Text(content: tag.text.clone(), color: tag.color)
                                            #(if i + 1 < visible_tags.len() {
                                                Some(element! { Text(content: " ", color: t.text) })
                                            } else { None })
                                        }
                                    }
                                }))
                            }
                            Text(content: " ", color: t.text)
                            View(
                                width: action_width as u32,
                                flex_direction: FlexDirection::Row,
                                justify_content: JustifyContent::FlexEnd,
                                overflow: Overflow::Hidden,
                            ) {
                                #(row.action_items.iter().enumerate().map(|(i, action)| {
                                    let btn_text = format!("[{}]", action.label);
                                    element! {
                                        View {
                                            Text(
                                                content: btn_text,
                                                color: if action.selected { t.action_selected } else { t.action },
                                                weight: if action.selected { Weight::Bold } else { Weight::Normal },
                                            )
                                            #(if i + 1 < row.action_items.len() {
                                                Some(element!{ Text(content: " ", color: t.action) })
                                            } else { None })
                                        }
                                    }
                                }))
                            }
                        }
                    }
                }))
                Splash(visible: splash_visible, color: t.text_muted, accent: Some(t.accent))
            }
            // Bottom panes (hidden when terminal too short)
            #(if bottom_h > 0 { Some(element! {
                View(height: bottom_h, flex_direction: FlexDirection::Row) {
                    View(
                        width: 50pct,
                        border_style: BorderStyle::Round,
                        border_color: t.border,
                        flex_direction: FlexDirection::Column,
                        overflow: Overflow::Hidden,
                    ) {
                        #(preview_lines.iter().map(|line| {
                            element! {
                                Text(content: line.text.clone(), color: line.color)
                            }
                        }))
                    }
                    View(
                        width: 50pct,
                        border_style: BorderStyle::Round,
                        border_color: t.border,
                        flex_direction: FlexDirection::Column,
                        overflow: Overflow::Hidden,
                    ) {
                        View(flex_direction: FlexDirection::Row) {
                            Text(content: "\u{2191}/\u{2193}  ", color: t.accent)
                            Text(content: "Navigate", color: t.text)
                        }
                        View(flex_direction: FlexDirection::Row) {
                            Text(content: "\u{2190}/\u{2192}  ", color: t.accent)
                            Text(content: "Select action", color: t.text)
                        }
                        View(flex_direction: FlexDirection::Row) {
                            Text(content: "Enter ", color: t.accent)
                            Text(content: "Open/execute", color: t.text)
                        }
                        View(flex_direction: FlexDirection::Row) {
                            Text(content: "Esc   ", color: t.accent)
                            Text(content: "Back/quit", color: t.text)
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
            }) } else { None })
        }
    }
}

pub fn run_picker(
    projects: &[Project],
    theme: Theme,
    scanned_at: Option<&str>,
    base_dir: PathBuf,
    api_url: String,
) -> Result<Option<Selection>> {
    let mut result: Option<Selection> = None;
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
                base_dir: base_dir,
                api_url: api_url,
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

fn build_root_actions(current: Option<&Project>) -> Vec<RootAction> {
    let mut actions = vec![
        RootAction::Cd,
        RootAction::Open,
        RootAction::Code,
        RootAction::Run,
        RootAction::Agent,
        RootAction::Copy,
    ];

    if let Some(project) = current {
        if has_deploy_urls(project) {
            actions.push(RootAction::Deploy);
        }
    }

    actions
}

fn build_subactions(kind: SubmenuKind, project: &Project) -> Vec<SelectionAction> {
    match kind {
        SubmenuKind::Open => vec![
            SelectionAction::Open(OpenAction::Iterm),
            SelectionAction::Open(OpenAction::Finder),
            SelectionAction::Open(OpenAction::Open),
        ],
        SubmenuKind::Code => vec![
            SelectionAction::Code(CodeAction::Claude),
            SelectionAction::Code(CodeAction::Codex),
            SelectionAction::Code(CodeAction::Opencode),
        ],
        SubmenuKind::Run => {
            if project.dev_command.is_some() {
                vec![SelectionAction::Run(RunAction::Dev)]
            } else {
                Vec::new()
            }
        }
        SubmenuKind::Agent => vec![
            SelectionAction::Agent(AgentAction::Claude),
            SelectionAction::Agent(AgentAction::Agents),
            SelectionAction::Agent(AgentAction::CopyClaudeToAgents),
            SelectionAction::Agent(AgentAction::CopyAgentsToClaude),
        ],
        SubmenuKind::Copy => {
            let mut actions = vec![SelectionAction::Copy(CopyAction::Path)];
            if project.dev_command.is_some() {
                actions.push(SelectionAction::Copy(CopyAction::DevCommand));
            }
            actions
        }
        SubmenuKind::Deploy => {
            let mut actions = Vec::new();
            if let Some(deploys) = &project.deploy {
                for deploy in deploys.iter().filter(|d| d.url.is_some()) {
                    let url = deploy.url.clone().unwrap_or_default();
                    if url.is_empty() {
                        continue;
                    }
                    actions.push(SelectionAction::Deploy(DeploySelection {
                        platform: deploy.platform.clone(),
                        url,
                    }));
                }
            }
            actions
        }
    }
}

fn build_action_items(
    submenu: Option<SubmenuKind>,
    current_root: Option<RootAction>,
    roots: &[RootAction],
    subactions: &[SelectionAction],
    root_idx: usize,
    sub_idx: usize,
) -> Vec<ActionItem> {
    match submenu {
        Some(kind) => subactions
            .iter()
            .enumerate()
            .map(|(i, action)| ActionItem {
                label: subaction_label(kind, action),
                selected: i == sub_idx,
            })
            .collect(),
        None => roots
            .iter()
            .enumerate()
            .map(|(i, action)| ActionItem {
                label: action.label().to_string(),
                selected: Some(*action) == current_root && i == root_idx,
            })
            .collect(),
    }
}

fn subaction_label(kind: SubmenuKind, action: &SelectionAction) -> String {
    match (kind, action) {
        (SubmenuKind::Open, SelectionAction::Open(OpenAction::Iterm)) => "iterm".to_string(),
        (SubmenuKind::Open, SelectionAction::Open(OpenAction::Finder)) => "finder".to_string(),
        (SubmenuKind::Open, SelectionAction::Open(OpenAction::Open)) => "open".to_string(),
        (SubmenuKind::Code, SelectionAction::Code(CodeAction::Claude)) => "claude".to_string(),
        (SubmenuKind::Code, SelectionAction::Code(CodeAction::Codex)) => "codex".to_string(),
        (SubmenuKind::Code, SelectionAction::Code(CodeAction::Opencode)) => "opencode".to_string(),
        (SubmenuKind::Run, SelectionAction::Run(RunAction::Dev)) => "dev".to_string(),
        (SubmenuKind::Agent, SelectionAction::Agent(AgentAction::Claude)) => "claude".to_string(),
        (SubmenuKind::Agent, SelectionAction::Agent(AgentAction::Agents)) => "agents".to_string(),
        (SubmenuKind::Agent, SelectionAction::Agent(AgentAction::CopyClaudeToAgents)) => {
            "cl->ag".to_string()
        }
        (SubmenuKind::Agent, SelectionAction::Agent(AgentAction::CopyAgentsToClaude)) => {
            "ag->cl".to_string()
        }
        (SubmenuKind::Copy, SelectionAction::Copy(CopyAction::Path)) => "path".to_string(),
        (SubmenuKind::Copy, SelectionAction::Copy(CopyAction::DevCommand)) => "devcmd".to_string(),
        (SubmenuKind::Deploy, SelectionAction::Deploy(sel)) => deploy_short(&sel.platform)
            .unwrap_or(&sel.platform)
            .to_string(),
        _ => "action".to_string(),
    }
}

fn build_tags(project: &Project, theme: Theme) -> Vec<Tag> {
    let mut tags = Vec::new();

    if let Some(status) = project.git.as_deref() {
        let color = match status {
            "clean" => COLOR_GREEN,
            "dirty" => COLOR_YELLOW,
            "error" => COLOR_RED,
            _ => theme.text_muted,
        };
        let text = if let Some(branch) = &project.git_branch {
            format!("● {}", truncate_branch(branch, 20))
        } else {
            "●".to_string()
        };
        tags.push(Tag { text, color });
    }

    if let Some(framework) = &project.framework {
        if framework != "unknown" {
            if let Some(short) = framework_short(framework) {
                let color = framework_color(framework, theme);
                tags.push(Tag {
                    text: format!("[{short}]"),
                    color,
                });
            }
        }
    }

    if let Some(runner) = &project.runner {
        let short = runner_short(runner).unwrap_or(runner);
        tags.push(Tag {
            text: format!("[{short}]"),
            color: theme.text,
        });
    }

    if let Some(deploys) = &project.deploy {
        for deploy in deploys {
            if let Some(short) = deploy_short(&deploy.platform) {
                tags.push(Tag {
                    text: format!("[{short}]"),
                    color: deploy_color(&deploy.platform, theme),
                });
            }
        }
    }

    tags
}

fn fit_tags(tags: &[Tag], max_width: usize) -> Vec<Tag> {
    let mut kept = tags.to_vec();
    while tags_len(&kept) > max_width && !kept.is_empty() {
        kept.pop();
    }
    kept
}

fn tags_len(tags: &[Tag]) -> usize {
    if tags.is_empty() {
        return 0;
    }
    let total: usize = tags.iter().map(|t| t.text.chars().count()).sum();
    total + tags.len().saturating_sub(1)
}

fn fit_text(text: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let len = text.chars().count();
    if len <= width {
        return text.to_string();
    }
    if width == 1 {
        return "\u{2026}".to_string();
    }
    let mut out = text
        .chars()
        .take(width.saturating_sub(1))
        .collect::<String>();
    out.push('\u{2026}');
    out
}

fn fit_action_items(items: Vec<ActionItem>, max_width: usize) -> Vec<ActionItem> {
    if items.is_empty() {
        return items;
    }
    let widths: Vec<usize> = items.iter().map(|a| a.label.chars().count() + 2).collect();
    let total: usize = widths.iter().sum::<usize>() + items.len() - 1;
    if total <= max_width {
        return items;
    }
    let sel = items.iter().position(|a| a.selected).unwrap_or(0);
    let mut lo = sel;
    let mut hi = sel;
    let mut used = widths[sel];
    loop {
        let mut grew = false;
        if hi + 1 < items.len() {
            let need = 1 + widths[hi + 1];
            if used + need <= max_width {
                hi += 1;
                used += need;
                grew = true;
            }
        }
        if lo > 0 {
            let need = widths[lo - 1] + 1;
            if used + need <= max_width {
                lo -= 1;
                used += need;
                grew = true;
            }
        }
        if !grew {
            break;
        }
    }
    items[lo..=hi].to_vec()
}

fn compute_column_widths(total: usize) -> (usize, usize, usize) {
    if total < 50 {
        let gaps = 2;
        let usable = total.saturating_sub(gaps);
        let action = (usable * 40 / 100).max(8);
        let name = usable.saturating_sub(action).max(4);
        return (name, 0, action);
    }
    let min_name = 18;
    let min_tag = 14;
    let min_action = 18;
    let mut name = clamp_col(total * 45 / 100, min_name, 60);
    let mut tag = clamp_col(total * 30 / 100, min_tag, 40);
    let mut action = clamp_col(total * 25 / 100, min_action, 44);
    let gaps = 3;

    let mut used = name + tag + action + gaps;
    if used > total {
        let mut over = used - total;
        reduce_col(&mut tag, min_tag, &mut over);
        reduce_col(&mut action, min_action, &mut over);
        reduce_col(&mut name, min_name, &mut over);
        if over > 0 {
            reduce_col(&mut tag, 4, &mut over);
            reduce_col(&mut action, 4, &mut over);
            reduce_col(&mut name, 4, &mut over);
        }
    } else {
        used = name + tag + action + gaps;
        if used + 2 < total {
            name += total - used;
        }
    }

    (name, tag, action)
}

fn clamp_col(value: usize, min: usize, max: usize) -> usize {
    value.max(min).min(max)
}

fn reduce_col(col: &mut usize, min: usize, over: &mut usize) {
    if *over == 0 || *col <= min {
        return;
    }
    let available = col.saturating_sub(min);
    let take = available.min(*over);
    *col -= take;
    *over -= take;
}

fn truncate_branch(branch: &str, max: usize) -> String {
    if branch.chars().count() <= max {
        branch.to_string()
    } else {
        let mut out = branch
            .chars()
            .take(max.saturating_sub(1))
            .collect::<String>();
        out.push('\u{2026}');
        out
    }
}

fn has_deploy_urls(project: &Project) -> bool {
    project
        .deploy
        .as_ref()
        .map(|d| {
            d.iter()
                .any(|info| info.url.as_ref().map(|u| !u.is_empty()).unwrap_or(false))
        })
        .unwrap_or(false)
}

fn framework_short(framework: &str) -> Option<&'static str> {
    match framework {
        "sveltekit" => Some("sk"),
        "svelte" => Some("sv"),
        "next" => Some("nx"),
        "nuxt" => Some("nu"),
        "astro" => Some("as"),
        "remix" => Some("rx"),
        "vite" => Some("vt"),
        "react" => Some("re"),
        "vue" => Some("vu"),
        "angular" => Some("ng"),
        "express" => Some("ex"),
        "fastify" => Some("fy"),
        "hono" => Some("ho"),
        "elysia" => Some("el"),
        "fastapi" => Some("fa"),
        "flask" => Some("fl"),
        "django" => Some("dj"),
        "streamlit" => Some("st"),
        "tauri" => Some("ta"),
        "electron" => Some("ec"),
        _ => None,
    }
}

fn framework_color(framework: &str, theme: Theme) -> Color {
    match framework {
        "sveltekit" | "svelte" => COLOR_ORANGE,
        "next" => theme.text,
        "nuxt" => COLOR_GREEN,
        "astro" => COLOR_PURPLE,
        "remix" => COLOR_BLUE,
        "vite" => COLOR_PURPLE,
        "react" => COLOR_BLUE,
        "vue" => COLOR_GREEN,
        "angular" => COLOR_RED,
        "express" | "fastify" => theme.text,
        "hono" => COLOR_ORANGE,
        "elysia" => COLOR_PURPLE,
        "fastapi" => COLOR_GREEN,
        "flask" => theme.text,
        "django" => COLOR_GREEN,
        "streamlit" => COLOR_RED,
        "tauri" => COLOR_YELLOW,
        "electron" => COLOR_BLUE,
        _ => theme.text_muted,
    }
}

fn runner_short(runner: &str) -> Option<&'static str> {
    match runner {
        "bun" => Some("bn"),
        "npm" => Some("np"),
        "yarn" => Some("yn"),
        "pnpm" => Some("pn"),
        "uv" => Some("uv"),
        _ => None,
    }
}

fn deploy_short(platform: &str) -> Option<&'static str> {
    match platform {
        "vercel" => Some("vc"),
        "render" => Some("rn"),
        "netlify" => Some("nf"),
        "docker" => Some("dk"),
        "github-actions" => Some("ci"),
        _ => None,
    }
}

fn deploy_color(platform: &str, theme: Theme) -> Color {
    match platform {
        "vercel" => theme.text,
        "render" => COLOR_PURPLE,
        "netlify" => COLOR_GREEN,
        "docker" => COLOR_BLUE,
        "github-actions" => COLOR_ORANGE,
        _ => COLOR_GRAY,
    }
}
