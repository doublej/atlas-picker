use crate::project::Project;
use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use nucleo::{Config, Matcher, Utf32Str};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Terminal,
};
use std::io;

struct App<'a> {
    projects: &'a [Project],
    filtered: Vec<(usize, u32)>, // (index, score)
    query: String,
    list_state: ListState,
    matcher: Matcher,
}

impl<'a> App<'a> {
    fn new(projects: &'a [Project]) -> Self {
        let filtered: Vec<_> = projects.iter().enumerate().map(|(i, _)| (i, 0u32)).collect();
        let mut list_state = ListState::default();
        if !filtered.is_empty() {
            list_state.select(Some(0));
        }
        Self {
            projects,
            filtered,
            query: String::new(),
            list_state,
            matcher: Matcher::new(Config::DEFAULT),
        }
    }

    fn update_filter(&mut self) {
        self.filtered = if self.query.is_empty() {
            self.projects.iter().enumerate().map(|(i, _)| (i, 0)).collect()
        } else {
            let mut buf = Vec::new();
            let pattern = nucleo::pattern::Pattern::parse(&self.query, nucleo::pattern::CaseMatching::Smart, nucleo::pattern::Normalization::Smart);
            let mut results: Vec<_> = self.projects.iter().enumerate().filter_map(|(i, p)| {
                let haystack = format!("{} {} {}", p.name, p.relative_path, p.description.as_deref().unwrap_or(""));
                pattern.score(Utf32Str::new(&haystack, &mut buf), &mut self.matcher).map(|score| (i, score))
            }).collect();
            results.sort_by(|a, b| b.1.cmp(&a.1));
            results
        };
        self.list_state.select((!self.filtered.is_empty()).then_some(0));
    }

    fn selected_project(&self) -> Option<&Project> {
        self.list_state
            .selected()
            .and_then(|i| self.filtered.get(i))
            .map(|(idx, _)| &self.projects[*idx])
    }

    fn move_selection(&mut self, delta: i32) {
        if self.filtered.is_empty() {
            return;
        }
        let len = self.filtered.len();
        let current = self.list_state.selected().unwrap_or(0) as i32;
        let new = (current + delta).rem_euclid(len as i32) as usize;
        self.list_state.select(Some(new));
    }
}

pub fn run_picker(projects: &[Project]) -> Result<Option<Project>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(projects);
    let result = run_loop(&mut terminal, &mut app);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

    result
}

fn run_loop(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, app: &mut App) -> Result<Option<Project>> {
    loop {
        terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(3), Constraint::Min(5), Constraint::Length(6)])
                .split(f.area());

            // Search input
            let input = Paragraph::new(app.query.as_str())
                .style(Style::default().fg(Color::Yellow))
                .block(Block::default().borders(Borders::ALL).title(format!(" Search ({} projects) ", app.filtered.len())));
            f.render_widget(input, chunks[0]);

            // Project list
            let items: Vec<ListItem> = app.filtered.iter().map(|(idx, _)| {
                let p = &app.projects[*idx];
                let mut spans = vec![Span::styled(&p.name, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))];
                if let Some(fw) = &p.framework {
                    spans.extend([Span::raw(" "), Span::styled(format!("[{}]", fw), Style::default().fg(Color::Magenta))]);
                }
                if let Some(t) = &p.project_type {
                    spans.extend([Span::raw(" "), Span::styled(t, Style::default().fg(Color::DarkGray))]);
                }
                ListItem::new(Line::from(spans))
            }).collect();

            let list = List::new(items)
                .block(Block::default().borders(Borders::ALL).title(" Projects "))
                .highlight_style(Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD))
                .highlight_symbol("▶ ");
            f.render_stateful_widget(list, chunks[1], &mut app.list_state);

            // Preview pane
            let dim = Style::default().fg(Color::DarkGray);
            let preview = if let Some(p) = app.selected_project() {
                let mut lines = vec![Line::from(vec![Span::styled("Path: ", dim), Span::raw(&p.path)])];
                if let Some(desc) = &p.description {
                    lines.push(Line::from(vec![Span::styled("Desc: ", dim), Span::raw(desc)]));
                }
                if let Some(branch) = &p.git_branch {
                    let color = match p.git.as_deref() {
                        Some("clean") => Color::Green, Some("dirty") => Color::Yellow, _ => Color::DarkGray,
                    };
                    lines.push(Line::from(vec![Span::styled("Git:  ", dim), Span::styled(branch, Style::default().fg(color))]));
                }
                if let Some(cmd) = &p.dev_command {
                    lines.push(Line::from(vec![Span::styled("Dev:  ", dim), Span::raw(format!("{} run {}", p.runner.as_deref().unwrap_or("npm"), cmd))]));
                }
                Paragraph::new(lines)
            } else {
                Paragraph::new("No project selected")
            };
            f.render_widget(preview.block(Block::default().borders(Borders::ALL).title(" Preview ")), chunks[2]);
        })?;

        if event::poll(std::time::Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                match key.code {
                    KeyCode::Esc => return Ok(None),
                    KeyCode::Enter => return Ok(app.selected_project().cloned()),
                    KeyCode::Up => app.move_selection(-1),
                    KeyCode::Down => app.move_selection(1),
                    KeyCode::Backspace => {
                        app.query.pop();
                        app.update_filter();
                    }
                    KeyCode::Char(c) => {
                        app.query.push(c);
                        app.update_filter();
                    }
                    _ => {}
                }
            }
        }
    }
}
