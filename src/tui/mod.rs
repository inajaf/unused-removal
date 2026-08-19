//! Terminal User Interface (TUI) using ratatui

use std::io;
use std::sync::Arc;
use std::time::{Duration, Instant};
use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Text},
    widgets::{Block, Borders, Clear, Gauge, List, ListItem, Paragraph, Row, Table, Wrap, TableState},
    Frame, Terminal,
};

use crate::config::Config;
use crate::scanner::{Scanner, Progress};
use crate::scanner_types::{Options, FileRecord, ScanError};
use crate::cache::{Cache, BoltCache, config_hash as cache_config_hash};
use crate::rules::{Engine, Finding, Category, Risk};
use crate::cleaner::{recycle_bin, hard_delete, DeleteResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AppState {
    Config,
    Scanning,
    Results,
    ConfirmDelete,
    Quitting,
}

#[derive(Debug, Clone)]
struct SelectedItem {
    path: String,
    size: i64,
}

pub fn run(config: Config) -> Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(config);
    let res = app.run(&mut terminal);

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(e) = res {
        eprintln!("Error: {}", e);
    }

    Ok(())
}

struct App {
    state: AppState,
    config: Config,
    
    // Config UI
    root_input: String,
    workers_input: String,
    follow_links: bool,
    use_cache: bool,
    check_duplicates: bool,
    protect_system: bool,
    config_focus: usize,
    
    // Scanning
    progress: Progress,
    scanner: Option<Scanner>,
    scan_handle: Option<std::thread::JoinHandle<anyhow::Result<(Vec<FileRecord>, Vec<ScanError>)>>>,
    scan_start: Option<Instant>,
    
    // Results
    findings: Vec<Finding>,
    filtered_findings: Vec<Finding>,
    selected: Vec<SelectedItem>,
    table_state: TableState,
    filter_category: Option<Category>,
    search: String,
    sort_desc: bool,
    
    // Delete confirmation
    delete_mode: Option<String>, // "recycle" or "hard"
    delete_paths: Vec<String>,
    
    // UI
    last_tick: Instant,
    status_msg: Option<(String, Style)>,
}

impl App {
    fn new(config: Config) -> Self {
        let mut app = Self {
            state: AppState::Config,
            config: config.clone(),
            root_input: config.root.clone(),
            workers_input: config.workers.to_string(),
            follow_links: config.follow_links,
            use_cache: config.use_cache,
            check_duplicates: config.check_duplicates,
            protect_system: config.protect_system,
            config_focus: 0,
            progress: Progress::new(),
            scanner: None,
            scan_handle: None,
            scan_start: None,
            findings: Vec::new(),
            filtered_findings: Vec::new(),
            selected: Vec::new(),
            table_state: TableState::default(),
            filter_category: None,
            search: String::new(),
            sort_desc: true,
            delete_mode: None,
            delete_paths: Vec::new(),
            last_tick: Instant::now(),
            status_msg: None,
        };
        app.table_state.select(Some(0));
        app
    }

    fn run(&mut self, terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
        loop {
            terminal.draw(|f| self.ui(f))?;

            // Handle events
            if event::poll(Duration::from_millis(50))? {
                if let Event::Key(key) = event::read()? {
                    if key.kind == KeyEventKind::Press {
                        self.handle_key(key.code);
                    }
                }
            }

            // Update progress during scanning
            if self.state == AppState::Scanning {
                if self.last_tick.elapsed() > Duration::from_millis(200) {
                    self.last_tick = Instant::now();
                    
                    // Check if scan is done
                    if let Some(handle) = &self.scan_handle {
                        if handle.is_finished() {
                            let handle = self.scan_handle.take().unwrap();
                            match handle.join() {
                                Ok(Ok((records, _errors))) => {
                                    let engine = Engine::new(Arc::new(self.config.clone()));
                                    let mut findings = engine.analyze(&records);
                                    if self.config.check_duplicates {
                                        let dups = engine.find_duplicates(&records);
                                        findings.extend(dups);
                                    }
                                    findings.sort_by(|a, b| b.size.cmp(&a.size));
                                    self.findings = findings;
                                    self.apply_filters();
                                    self.state = AppState::Results;
                                    self.status_msg = Some((
                                        format!("Found {} files, {} findings", records.len(), self.findings.len()),
                                        Style::default().fg(Color::Green),
                                    ));
                                }
                                Ok(Err(e)) => {
                                    self.state = AppState::Config;
                                    self.status_msg = Some((format!("Scan error: {}", e), Style::default().fg(Color::Red)));
                                }
                                Err(_) => {
                                    self.state = AppState::Config;
                                    self.status_msg = Some(("Scan panicked".to_string(), Style::default().fg(Color::Red)));
                                }
                            }
                        }
                    }
                }
            }

            // Check quit
            if self.state == AppState::Quitting {
                break;
            }
        }
        Ok(())
    }

    fn handle_key(&mut self, key: KeyCode) {
        match self.state {
            AppState::Config => self.handle_config_key(key),
            AppState::Scanning => self.handle_scanning_key(key),
            AppState::Results => self.handle_results_key(key),
            AppState::ConfirmDelete => self.handle_confirm_key(key),
            _ => {}
        }
    }

    fn handle_config_key(&mut self, key: KeyCode) {
        match key {
            KeyCode::Char('q') | KeyCode::Esc => self.state = AppState::Quitting,
            KeyCode::Tab | KeyCode::Down => self.config_focus = (self.config_focus + 1) % 6,
            KeyCode::BackTab | KeyCode::Up => self.config_focus = (self.config_focus + 5) % 6,
            KeyCode::Enter => {
                if self.config_focus == 5 {
                    self.start_scan();
                } else {
                    self.config_focus = (self.config_focus + 1) % 6;
                }
            }
            KeyCode::Char(' ') => {
                match self.config_focus {
                    2 => self.follow_links = !self.follow_links,
                    3 => self.use_cache = !self.use_cache,
                    4 => self.check_duplicates = !self.check_duplicates,
                    5 => self.start_scan(),
                    _ => {}
                }
            }
            KeyCode::Char(c) => {
                match self.config_focus {
                    0 => self.root_input.push(c),
                    1 => if c.is_ascii_digit() { self.workers_input.push(c) },
                    _ => {}
                }
            }
            KeyCode::Backspace => {
                match self.config_focus {
                    0 => { self.root_input.pop(); }
                    1 => { self.workers_input.pop(); }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    fn handle_scanning_key(&mut self, key: KeyCode) {
        match key {
            KeyCode::Char('q') | KeyCode::Esc => {
                if let Some(s) = &self.scanner {
                    s.stop();
                }
                self.state = AppState::Config;
            }
            KeyCode::Char('s') => {
                if let Some(s) = &self.scanner {
                    s.stop();
                }
                self.state = AppState::Config;
                self.status_msg = Some(("Scan stopped".to_string(), Style::default().fg(Color::Yellow)));
            }
            _ => {}
        }
    }

    fn handle_results_key(&mut self, key: KeyCode) {
        match key {
            KeyCode::Char('q') | KeyCode::Esc => self.state = AppState::Quitting,
            KeyCode::Char('r') => self.state = AppState::Config,
            KeyCode::Char('t') => self.confirm_delete("recycle"),
            KeyCode::Char('x') => self.confirm_delete("hard"),
            KeyCode::Char('c') => self.cycle_category_filter(),
            KeyCode::Char('/') => {
                self.status_msg = Some(("Type to search, Esc to clear".to_string(), Style::default().fg(Color::Cyan)));
            }
            KeyCode::Char(' ') => self.toggle_selection(),
            KeyCode::Char(c) => {
                self.search.push(c);
                self.apply_filters();
            }
            KeyCode::Backspace => {
                self.search.pop();
                self.apply_filters();
            }
            KeyCode::Up => self.previous_row(),
            KeyCode::Down => self.next_row(),
            KeyCode::Enter => self.toggle_selection(),
            _ => {}
        }
    }

    fn handle_confirm_key(&mut self, key: KeyCode) {
        match key {
            KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => self.execute_delete(),
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                self.state = AppState::Results;
                self.delete_mode = None;
                self.delete_paths.clear();
            }
            _ => {}
        }
    }

    fn start_scan(&mut self) {
        if self.root_input.trim().is_empty() {
            self.status_msg = Some(("Please enter a root path".to_string(), Style::default().fg(Color::Red)));
            return;
        }

        self.config.root = self.root_input.clone();
        if let Ok(w) = self.workers_input.parse() {
            self.config.workers = w;
        }
        self.config.follow_links = self.follow_links;
        self.config.use_cache = self.use_cache;
        self.config.check_duplicates = self.check_duplicates;
        self.config.protect_system = self.protect_system;

        self.progress = Progress::new();
        let opts = Options {
            workers: self.config.workers,
            follow_links: self.config.follow_links,
            exclude: self.config.exclude_dirs.clone(),
            exclude_pref: self.config.exclude_prefix.clone(),
        };

        let cache: Option<Arc<dyn Cache>> = if self.config.use_cache {
            let hash = cache_config_hash(&opts);
            BoltCache::new("unused-removal", &hash).ok().map(|c| Arc::new(c) as Arc<dyn Cache>)
        } else {
            None
        };

        let scanner = Scanner::new(opts, self.progress.clone(), cache);
        let root = self.config.root.clone();

        self.findings.clear();
        self.filtered_findings.clear();
        self.selected.clear();
        self.table_state.select(Some(0));
        self.state = AppState::Scanning;
        self.scan_start = Some(Instant::now());
        self.status_msg = None;

        self.scan_handle = Some(std::thread::spawn(move || scanner.walk(&root)));
    }

    fn apply_filters(&mut self) {
        self.filtered_findings = self.findings.iter()
            .filter(|f| {
                if let Some(cat) = self.filter_category {
                    if f.category != cat { return false; }
                }
                if !self.search.is_empty() {
                    let s = self.search.to_lowercase();
                    if !f.path.to_lowercase().contains(&s) && !f.reason.to_lowercase().contains(&s) {
                        return false;
                    }
                }
                true
            })
            .cloned()
            .collect();

        // Sort (by size; ascending/descending toggle)
        self.filtered_findings.sort_by(|a, b| {
            let ord = a.size.cmp(&b.size);
            if self.sort_desc { ord.reverse() } else { ord }
        });

        if self.table_state.selected().unwrap_or(0) >= self.filtered_findings.len() {
            if !self.filtered_findings.is_empty() {
                self.table_state.select(Some(self.filtered_findings.len() - 1));
            } else {
                self.table_state.select(None);
            }
        }
    }

    fn cycle_category_filter(&mut self) {
        let categories = [
            None,
            Some(Category::Huge),
            Some(Category::Large),
            Some(Category::Junk),
            Some(Category::OldLog),
            Some(Category::StaleInstall),
            Some(Category::Stale),
            Some(Category::Duplicate),
        ];
        let current = self.filter_category;
        let idx = categories.iter().position(|c| *c == current).unwrap_or(0);
        self.filter_category = categories[(idx + 1) % categories.len()];
        self.apply_filters();
    }

    fn previous_row(&mut self) {
        let i = self.table_state.selected().unwrap_or(0);
        if i > 0 {
            self.table_state.select(Some(i - 1));
        }
    }

    fn next_row(&mut self) {
        let i = self.table_state.selected().unwrap_or(0);
        if i + 1 < self.filtered_findings.len() {
            self.table_state.select(Some(i + 1));
        }
    }

    fn toggle_selection(&mut self) {
        if let Some(i) = self.table_state.selected() {
            if let Some(finding) = self.filtered_findings.get(i) {
                let path = finding.path.clone();
                if let Some(pos) = self.selected.iter().position(|s| s.path == path) {
                    self.selected.remove(pos);
                } else {
                    self.selected.push(SelectedItem { path, size: finding.size });
                }
            }
        }
    }

    fn confirm_delete(&mut self, mode: &str) {
        if self.selected.is_empty() {
            self.status_msg = Some(("Nothing selected. Use Space to select items.".to_string(), Style::default().fg(Color::Yellow)));
            return;
        }
        self.delete_mode = Some(mode.to_string());
        self.delete_paths = self.selected.iter().map(|s| s.path.clone()).collect();
        self.state = AppState::ConfirmDelete;
    }

    fn execute_delete(&mut self) {
        let mode = self.delete_mode.take().unwrap_or("recycle".to_string());
        let paths = self.delete_paths.drain(..).collect::<Vec<_>>();
        
        let result: Result<DeleteResult> = if mode == "hard" {
            hard_delete(&paths)
        } else {
            recycle_bin(&paths)
        };

        match result {
            Ok(res) => {
                let deleted_set: std::collections::HashSet<_> = res.deleted.iter().cloned().collect();
                self.findings.retain(|f| !deleted_set.contains(&f.path));
                self.apply_filters();
                self.selected.clear();
                
                self.status_msg = Some((
                    format!("Deleted {} files ({})", res.deleted.len(), format_bytes(res.total_bytes)),
                    Style::default().fg(Color::Green),
                ));
                if !res.failed.is_empty() {
                    self.status_msg = Some((
                        format!("Deleted {} files, {} failed", res.deleted.len(), res.failed.len()),
                        Style::default().fg(Color::Yellow),
                    ));
                }
            }
            Err(e) => {
                self.status_msg = Some((format!("Delete error: {}", e), Style::default().fg(Color::Red)));
            }
        }
        
        self.state = AppState::Results;
    }

    fn ui(&mut self, f: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),  // Header
                Constraint::Min(10),    // Content
                Constraint::Length(3),  // Status/Help
            ])
            .split(f.size());

        self.render_header(f, chunks[0]);
        
        match self.state {
            AppState::Config => self.render_config(f, chunks[1]),
            AppState::Scanning => self.render_scanning(f, chunks[1]),
            AppState::Results => self.render_results(f, chunks[1]),
            AppState::ConfirmDelete => {
                self.render_results(f, chunks[1]);
                self.render_confirm_dialog(f);
            }
            _ => {}
        }
        
        self.render_status(f, chunks[2]);
    }

    fn render_header(&self, f: &mut Frame, area: Rect) {
        let title = Paragraph::new("🗂 unused-removal")
            .style(Style::default().fg(Color::LightBlue).add_modifier(Modifier::BOLD))
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::DarkGray)));
        f.render_widget(title, area);
    }

    fn render_config(&mut self, f: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Min(5),
            ])
            .split(area);

        let focus_style = Style::default().fg(Color::LightBlue).add_modifier(Modifier::BOLD);
        let normal_style = Style::default().fg(Color::White);

        // Root path
        let style = if self.config_focus == 0 { focus_style } else { normal_style };
        let root = Paragraph::new(self.root_input.as_str())
            .style(style)
            .block(Block::default().title("Root Path").borders(Borders::ALL));
        f.render_widget(root, chunks[0]);

        // Workers
        let style = if self.config_focus == 1 { focus_style } else { normal_style };
        let workers = Paragraph::new(self.workers_input.as_str())
            .style(style)
            .block(Block::default().title("Workers (0=auto)").borders(Borders::ALL));
        f.render_widget(workers, chunks[1]);

        // Checkboxes
        let checkboxes = vec![
            (2, "Follow Links", self.follow_links),
            (3, "Use Cache", self.use_cache),
            (4, "Find Duplicates", self.check_duplicates),
        ];
        for (idx, (focus, label, value)) in checkboxes.iter().enumerate() {
            let style = if self.config_focus == *focus { focus_style } else { normal_style };
            let checkbox = format!("[{}] {}", if *value { "✓" } else { " " }, label);
            let p = Paragraph::new(checkbox).style(style).block(Block::default().borders(Borders::ALL));
            f.render_widget(p, chunks[2 + idx]);
        }

        // Scan button
        let style = if self.config_focus == 5 { 
            Style::default().fg(Color::Black).bg(Color::LightBlue).add_modifier(Modifier::BOLD)
        } else { 
            Style::default().fg(Color::White).bg(Color::Blue) 
        };
        let btn = Paragraph::new("▶ Start Scan")
            .style(style)
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL));
        f.render_widget(btn, chunks[5]);

        // Help
        let help = Paragraph::new("Tab/↓: Next field  Enter: Activate  Space: Toggle  q: Quit")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);
        f.render_widget(help, chunks[6]);
    }

    fn render_scanning(&mut self, f: &mut Frame, area: Rect) {
        let snap = self.progress.snapshot();
        
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(7),  // Progress ring + stats
                Constraint::Length(3),  // Current path
                Constraint::Min(5),     // Recent files
            ])
            .split(area);

        // Progress ring and stats
        let stats_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(30), // Ring
                Constraint::Min(0),     // Stats grid
            ])
            .split(chunks[0]);

        // Progress ring (ASCII)
        let percent = if snap.percent >= 0.0 { snap.percent as u16 } else { 0 };
        let gauge = Gauge::default()
            .block(Block::default().title("Progress").borders(Borders::ALL))
            .gauge_style(Style::default().fg(Color::LightBlue).bg(Color::DarkGray))
            .percent(percent.min(100))
            .label(format!("{:.0}%", snap.percent.max(0.0)));
        f.render_widget(gauge, stats_chunks[0]);

        // Stats grid
        let stat_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(20),
                Constraint::Percentage(20),
                Constraint::Percentage(20),
                Constraint::Percentage(20),
                Constraint::Percentage(20),
            ])
            .split(stats_chunks[1]);

        let stats = vec![
            ("Files", format_number(snap.files)),
            ("Dirs", format_number(snap.dirs)),
            ("Size", format_bytes(snap.bytes as u64)),
            ("Rate", format!("{:.0}/s", snap.rate_fps)),
            ("Cached", format_number(snap.cached)),
        ];
        for (i, (label, value)) in stats.iter().enumerate() {
            let p = Paragraph::new(format!("{}\n{}", label, value))
                .style(Style::default().fg(Color::White))
                .alignment(Alignment::Center)
                .block(Block::default().borders(Borders::ALL));
            f.render_widget(p, stat_chunks[i]);
        }

        // Current directory
        let current = Paragraph::new(snap.current.as_str())
            .style(Style::default().fg(Color::Cyan))
            .block(Block::default().title("Current Directory").borders(Borders::ALL))
            .wrap(Wrap { trim: true });
        f.render_widget(current, chunks[1]);

        // Recent files
        let recent_items: Vec<ListItem> = snap.recent.iter().rev().take(10)
            .map(|p| ListItem::new(Line::from(p.as_str()).style(Style::default().fg(Color::DarkGray))))
            .collect();
        let recent = List::new(recent_items)
            .block(Block::default().title("Recent Files").borders(Borders::ALL));
        f.render_widget(recent, chunks[2]);
    }

    fn render_results(&mut self, f: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),  // Filters
                Constraint::Min(5),     // Table
                Constraint::Length(3),  // Summary
            ])
            .split(area);

        // Filters
        let filter_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(30),
                Constraint::Percentage(70),
            ])
            .split(chunks[0]);

        let cat_filter = self.filter_category.map(|c| format!("{:?}", c)).unwrap_or("All".to_string());
        let filter_text = format!("Category: {}  Search: {}", cat_filter, if self.search.is_empty() { "(none)" } else { &self.search });
        let filters = Paragraph::new(filter_text)
            .style(Style::default().fg(Color::Cyan))
            .block(Block::default().title("Filters [c: category, /: search]").borders(Borders::ALL));
        f.render_widget(filters, filter_chunks[0]);

        let sel_info = if self.selected.is_empty() {
            "Nothing selected".to_string()
        } else {
            let total_size: u64 = self.selected.iter().map(|s| s.size as u64).sum();
            format!("Selected: {} ({})", self.selected.len(), format_bytes(total_size))
        };
        let sel = Paragraph::new(sel_info)
            .style(Style::default().fg(Color::Yellow))
            .alignment(Alignment::Right)
            .block(Block::default().borders(Borders::ALL));
        f.render_widget(sel, filter_chunks[1]);

        // Table
        let rows: Vec<Row> = self.filtered_findings.iter().enumerate().map(|(i, f)| {
            let style = if self.table_state.selected() == Some(i) {
                Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            
            let selected_mark = if self.selected.iter().any(|s| s.path == f.path) { "● " } else { "  " };
            let risk_color = match f.risk {
                Risk::Safe => Color::Green,
                Risk::Caution => Color::Yellow,
                Risk::Protected => Color::Red,
            };
            
            Row::new(vec![
                format!("{}{}", selected_mark, format_bytes(f.size as u64)),
                format!("{:?}", f.category),
                format!("{:?}", f.risk).fg(risk_color).to_string(),
                f.path.clone(),
            ]).style(style)
        }).collect();

        let widths = [
            Constraint::Length(15),
            Constraint::Length(15),
            Constraint::Length(10),
            Constraint::Min(30),
        ];

        let table = Table::new(rows, widths)
            .header(Row::new(vec!["Size", "Category", "Risk", "Path"])
                .style(Style::default().fg(Color::LightBlue).add_modifier(Modifier::BOLD))
                .bottom_margin(1))
            .block(Block::default().title("Findings [↑↓: navigate, Space: select, t: recycle, x: hard delete, c: filter, r: back]").borders(Borders::ALL))
            .highlight_style(Style::default().bg(Color::DarkGray))
            .column_spacing(2);
        f.render_stateful_widget(table, chunks[1], &mut self.table_state);

        // Summary
        let mut summary = String::new();
        use std::collections::HashMap;
        let mut by_cat: HashMap<Category, usize> = HashMap::new();
        for f in &self.findings {
            *by_cat.entry(f.category).or_default() += 1;
        }
        for cat in [Category::Huge, Category::Large, Category::Junk, Category::OldLog, Category::StaleInstall, Category::Stale, Category::Duplicate] {
            if let Some(count) = by_cat.get(&cat) {
                summary.push_str(&format!("{:?}: {}  ", cat, count));
            }
        }
        let summary_p = Paragraph::new(summary)
            .style(Style::default().fg(Color::White))
            .block(Block::default().title("Summary").borders(Borders::ALL));
        f.render_widget(summary_p, chunks[2]);
    }

    fn render_confirm_dialog(&mut self, f: &mut Frame) {
        let area = centered_rect(60, 30, f.size());
        f.render_widget(Clear, area);

        let mode = self.delete_mode.as_deref().unwrap_or("recycle");
        let is_hard = mode == "hard";
        let total_size: u64 = self.delete_paths.iter()
            .filter_map(|p| self.findings.iter().find(|f| &f.path == p).map(|f| f.size as u64))
            .sum();

        let text = if is_hard {
            vec![
                Line::from("⚠️  PERMANENT DELETE"),
                Line::from(""),
                Line::from(format!("Delete {} files ({})?", self.delete_paths.len(), format_bytes(total_size))),
                Line::from(""),
                Line::from("⚠️ THIS ACTION CANNOT BE UNDONE!".fg(Color::Red).add_modifier(Modifier::BOLD)),
                Line::from("Files will NOT go to Recycle Bin."),
                Line::from(""),
                Line::from("Press Y/Enter to confirm, N/Esc to cancel"),
            ]
        } else {
            vec![
                Line::from("🗑  Move to Recycle Bin"),
                Line::from(""),
                Line::from(format!("Move {} files ({}) to Recycle Bin?", self.delete_paths.len(), format_bytes(total_size))),
                Line::from(""),
                Line::from("✅ Files can be restored from Recycle Bin.".fg(Color::Green)),
                Line::from(""),
                Line::from("Press Y/Enter to confirm, N/Esc to cancel"),
            ]
        };

        let dialog = Paragraph::new(Text::from(text))
            .style(Style::default().fg(Color::White))
            .alignment(Alignment::Center)
            .block(Block::default().title("Confirm").borders(Borders::ALL).border_style(Style::default().fg(Color::Red)));
        f.render_widget(dialog, area);
    }

    fn render_status(&self, f: &mut Frame, area: Rect) {
        let (msg, style) = self.status_msg.clone().unwrap_or_else(|| {
            match self.state {
                AppState::Config => ("Tab: navigate  Enter: scan  q: quit".to_string(), Style::default().fg(Color::DarkGray)),
                AppState::Scanning => ("s: stop scan  q: quit".to_string(), Style::default().fg(Color::DarkGray)),
                AppState::Results => ("↑↓: navigate  Space: select  t: recycle  x: hard delete  c: filter  r: config  q: quit".to_string(), Style::default().fg(Color::DarkGray)),
                AppState::ConfirmDelete => ("Y: confirm  N: cancel".to_string(), Style::default().fg(Color::Red)),
                _ => ("".to_string(), Style::default()),
            }
        });
        
        let p = Paragraph::new(msg).style(style).alignment(Alignment::Center);
        f.render_widget(p, area);
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

fn format_bytes(bytes: u64) -> String {
    const UNIT: u64 = 1024;
    if bytes < UNIT {
        return format!("{} B", bytes);
    }
    let mut div = UNIT;
    let mut exp = 0;
    let mut n = bytes / UNIT;
    while n >= UNIT {
        div *= UNIT;
        exp += 1;
        n /= UNIT;
    }
    format!("{:.1} {}iB", bytes as f64 / div as f64, "KMGTPE".chars().nth(exp).unwrap())
}

fn format_number(n: i64) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, ch) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(' ');
        }
        result.push(ch);
    }
    result.chars().rev().collect()
}