use std::io;
use std::path::Path;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use crossterm::event::{Event, KeyCode, read};
use ratatui_interact::components::{Button, ButtonState, ButtonVariant};
use ratatui_interact::state::FocusManager;
use ratatui::{crossterm, widgets::FrameExt};
use ratatui::{
    layout::{Rect, Direction, Constraint, Flex, Layout},
    style::{Stylize, Color, Style, Modifier},
    symbols::border,
    text::Line,
    widgets::{Block, Paragraph, Borders, Padding, Table, Row, TableState},
    DefaultTerminal, Frame,
};
use ratatui_explorer::{FileExplorerBuilder, Theme};
use akhtabooti_core::{search_directory, search_file, FilePIIs};

fn center(area: Rect, width: Constraint, height: Constraint) -> Rect {
    let [area] = Layout::horizontal([width]).flex(Flex::Center).areas(area);
    let [area] = Layout::vertical([height]).flex(Flex::Center).areas(area);
    area
}

fn center_horizontal(area: Rect, width: Constraint) -> Rect {
    let [area] = Layout::horizontal([width]).flex(Flex::Center).areas(area);
    area
}

fn app_block(instructions: Option<Line>) -> Block {
    let title = Line::from(" akhtabooti ".bold());
    let block = Block::bordered()
        .title(title.centered())
        .border_set(border::THICK);

    match instructions {
        Some(instructions) => block.title_bottom(instructions.centered()),
        None => block,
    }
}

fn render_centered_button(frame: &mut Frame, button: Button, row: Rect) {
    let width = button.min_width();
    let area = center_horizontal(row, Constraint::Length(width));
    frame.render_widget(button, area);
}

fn results_table(results: &[&FilePIIs]) -> Table<'static> {
    let header = Row::new(["file path", "email accounts", "phone numbers", "other piis"])
        .style(Style::new().bold())
        .bottom_margin(1);


    let rows: Vec<Row> = results.iter().map(|pii| {
        let total = severity_total(pii);
        Row::new([
            pii.filename.clone(),
            pii.email_accounts.len().to_string(),
            pii.phone_numbers.len().to_string(),
            pii.other_piis.len().to_string(),
        ]).style(Style::new().fg(severity_color(total)))
    }).collect();

    let widths = [
        Constraint::Percentage(70),
        Constraint::Percentage(10),
        Constraint::Percentage(10),
        Constraint::Percentage(10),
    ];

    Table::new(rows, widths)
        .header(header)
        .column_spacing(1)
        .style(Color::White)
        .row_highlight_style(Style::new().add_modifier(Modifier::REVERSED).bold())
        .column_highlight_style(Style::default())
        .cell_highlight_style(Style::new().add_modifier(Modifier::REVERSED).bold())
        .highlight_symbol("> ")
}

fn severity_total(pii: &FilePIIs) -> usize {
    pii.email_accounts.len() + pii.phone_numbers.len() + pii.other_piis.len()
}

fn severity_color(total: usize) -> Color {
    match total {
        0 => Color::Green,
        1..=4 => Color::Yellow,
        5..=9 => Color::Rgb(255, 165, 0),
        _ => Color::Red,
    }
}

fn expanded_result(pii: &akhtabooti_core::FilePIIs) -> Vec<Line<'_>> {
    const MAX_LINES: usize = 8;

    let results: Vec<Line> = [
        (!pii.email_accounts.is_empty()).then(|| labeled_section("email_accounts", &pii.email_accounts, MAX_LINES)),
        (!pii.phone_numbers.is_empty()).then(|| labeled_section("phone_numbers", &pii.phone_numbers, MAX_LINES)),
        (!pii.other_piis.is_empty()).then(|| labeled_section("other_piis", &pii.other_piis, MAX_LINES)),
    ].into_iter().flatten().flatten().collect();

    results
}

fn labeled_section<'a>(
    label: &'a str,
    items: impl IntoIterator<Item = &'a String>,
    max: usize,
) -> Vec<Line<'a>> {
    let mut lines = vec![Line::from(format!("{label}: ").bold())];
    lines.extend(items.into_iter().take(max).map(|s| Line::from(format!("- {s}"))));
    lines.push(Line::from(""));
    lines
}

#[derive(Clone, PartialEq, Eq, Hash)]
enum FocusTarget {
    Explore,
    Scan,
    Exit,
}

pub struct App {
    exit: bool,
    focus: FocusManager<FocusTarget>,
    explore_btn: ButtonState,
    scan_btn: ButtonState,
    exit_btn: ButtonState,
    selected_path: Option<String>,
    throbber_state: throbber_widgets_tui::ThrobberState,
    results: Vec<FilePIIs>,
}

impl App {
    pub fn run(&mut self, terminal: &mut DefaultTerminal) -> io::Result<()> {
        self.sync_button_focus();
        self.main_menu(terminal)
    }

    fn sync_button_focus(&mut self) {
        self.explore_btn.focused = self.focus.is_focused(&FocusTarget::Explore);
        self.scan_btn.focused = self.focus.is_focused(&FocusTarget::Scan);
        self.exit_btn.focused = self.focus.is_focused(&FocusTarget::Exit);
    }

    fn exit(&mut self) {
        self.exit = true;
    }

    fn main_menu(&mut self, terminal: &mut DefaultTerminal) -> io::Result<()> {
        while !self.exit {
            self.sync_button_focus();
            terminal.draw(|frame| {
                let instructions = Line::from(vec![
                    " (↑) ".bold(), "prev |".into(),
                    " (↓) ".bold(), "next |".into(),
                    " (Enter) ".bold(), "select |".into(),
                    " (q) ".bold(), "quit ".into(),
                ]);
                let block = app_block(Some(instructions));
                let inner = block.inner(frame.area());
                frame.render_widget(block, frame.area());

                let explore_label = self.selected_path.clone()
                    .unwrap_or_else(|| "[ Select path to scan ]".to_string());

                let explore_button = Button::new(&explore_label, &self.explore_btn)
                    .variant(ButtonVariant::SingleLine);
                let scan_button = Button::new("[ Scan ]", &self.scan_btn)
                    .variant(ButtonVariant::SingleLine);
                let exit_button = Button::new("[ Exit ]", &self.exit_btn)
                    .variant(ButtonVariant::SingleLine);

                let show_scan = self.selected_path.is_some();
                let row_count = if show_scan { 5 } else { 3 };

                let content = center(inner, Constraint::Fill(1), Constraint::Length(row_count));
                let rows = Layout::vertical(vec![Constraint::Length(1); row_count as usize]).split(content);

                render_centered_button(frame, explore_button, rows[0]);
                if show_scan {
                    render_centered_button(frame, scan_button, rows[2]);
                    render_centered_button(frame, exit_button, rows[4]);
                } else {
                    render_centered_button(frame, exit_button, rows[2]);
                }
            })?;

            if let Event::Key(key) = read()? {
                match key.code {
                    KeyCode::Char('q') => self.exit(),
                    KeyCode::Down => {
                        self.focus.next();
                        if self.selected_path.is_none() && self.focus.is_focused(&FocusTarget::Scan) {
                            self.focus.next();
                        }
                    }
                    KeyCode::Up => {
                        self.focus.prev();
                        if self.selected_path.is_none() && self.focus.is_focused(&FocusTarget::Scan) {
                            self.focus.prev();
                        }
                    }
                    KeyCode::Enter => {
                        if self.focus.is_focused(&FocusTarget::Exit) {
                            self.exit();
                        } else if self.focus.is_focused(&FocusTarget::Scan) {
                            if let Some(path) = self.selected_path.clone() {
                                self.scanning(terminal, path)?;
                                self.scan_results(terminal)?;
                            }
                        } else if self.focus.is_focused(&FocusTarget::Explore) {
                            let path = self.open_file_explorer(terminal)?;
                            if !path.is_empty() {
                                self.selected_path = Some(path);
                            }
                            self.focus.next();
                        }
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }

    fn open_file_explorer(&mut self, terminal: &mut DefaultTerminal) -> io::Result<String> {
        let mut selected = String::new();
        let theme = Theme::default().add_default_title();
        let mut file_explorer = FileExplorerBuilder::build_with_theme(theme)?;

        loop {
            terminal.draw(|frame| {
                let instructions = Line::from(vec![
                    " (↑) ".bold(), "prev |".into(),
                    " (↓) ".bold(), "next |".into(),
                    " (q) ".bold(), "quit |".into(),
                    " (enter) ".bold(), "enter |".into(),
                    " (space) ".bold(), "select |".into(),
                ]);
                let block = app_block(Some(instructions)).padding(Padding::new(1, 1, 0, 0));
                let inner = block.inner(frame.area());
                frame.render_widget(block, frame.area());

                frame.render_widget_ref(file_explorer.widget(), inner);
            })?;

            let event = read()?;
            if let Event::Key(key) = &event {
                match key.code {
                    KeyCode::Char('q') => break,
                    KeyCode::Char(' ') => {
                        selected = file_explorer.current().path.to_string_lossy().to_string();
                        break;
                    }
                    _ => {}
                }
            }
            file_explorer.handle(&event)?;
        }
        Ok(selected)
    }

    fn on_tick(&mut self) {
        self.throbber_state.calc_next();
    }

    fn scan_path(path: String) -> Vec<FilePIIs> {
        if Path::new(&path).is_dir() {
            search_directory(&path).unwrap()
        } else {
            vec![search_file(&path).unwrap()]
        }
    }

    fn scanning(&mut self, terminal: &mut DefaultTerminal, path: String) -> io::Result<()> {
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let _ = tx.send(Self::scan_path(path));
        });

        loop {
            if let Ok(results) = rx.try_recv() {
                self.results = results;
                break;
            }

            self.on_tick();

            terminal.draw(|frame| {
                let block = app_block(None);
                let inner = block.inner(frame.area());
                frame.render_widget(block, frame.area());

                let label = "Scanning...";
                let width = label.chars().count() as u16 + 4;
                let area = center(inner, Constraint::Length(width), Constraint::Length(1));

                let throbber = throbber_widgets_tui::Throbber::default()
                    .label("Scanning...")
                    .style(Style::default().fg(Color::Cyan))
                    .throbber_style(Style::default().fg(Color::Red).bold())
                    .throbber_set(throbber_widgets_tui::BLACK_CIRCLE)
                    .use_type(throbber_widgets_tui::WhichUse::Spin);
                frame.render_stateful_widget(throbber, area, &mut self.throbber_state);
            })?;

            thread::sleep(Duration::from_millis(80));
        }
        Ok(())
    }

    fn scan_results(&mut self, terminal: &mut DefaultTerminal) -> io::Result<()> {
        let nonempty_results: Vec<&FilePIIs> = self.results.iter()
            .filter(|p| !p.email_accounts.is_empty() || !p.phone_numbers.is_empty() || !p.other_piis.is_empty())
            .collect();

        let all_results: Vec<&FilePIIs> = self.results.iter().collect();

        let mut show_all = false;

        let mut table_state = TableState::default();
        table_state.select_first();
        table_state.select_first_column();

        let mut expand = false;

        loop {
            let visible: &[&FilePIIs] = if show_all { &all_results } else { &nonempty_results };

            let mut sorted_results: Vec<&FilePIIs> = visible.to_vec();
            sorted_results.sort_by_key(|p| std::cmp::Reverse(severity_total(p)));

            terminal.draw(|frame| {
                let instructions = Line::from(vec![
                    " (↑) ".bold(), "prev |".into(),
                    " (↓) ".bold(), "next |".into(),
                    " (Enter) ".bold(), "expand |".into(),
                    " (h) ".bold(), "toggle empty |".into(),
                    " (q) ".bold(), "quit ".into(),
                ]);
                let block = app_block(Some(instructions));
                let inner = block.inner(frame.area());
                frame.render_widget(block, frame.area());

                if expand {
                    let split = Layout::default()
                        .direction(Direction::Horizontal)
                        .constraints(vec![
                            Constraint::Percentage(69),
                            Constraint::Percentage(2),
                            Constraint::Percentage(29)
                        ])
                        .split(inner);
    
                    let table = results_table(&sorted_results);
                    frame.render_stateful_widget(table, split[0], &mut table_state);
    
                    if let Some(pii) = table_state.selected().and_then(|i| visible.get(i).copied()) {
                        let item_details = expanded_result(pii);
                        frame.render_widget(
                            Paragraph::new(item_details)
                                .block(Block::new().bold().fg(Color::White).borders(Borders::ALL)),
                            split[2],
                        )
                    }
                } else {
                    let table = results_table(&sorted_results);
                    frame.render_stateful_widget(table, inner, &mut table_state);
                }
            })?;

            if let Event::Key(key) = read()? {
                match key.code {
                    KeyCode::Char('q') => return Ok(()),
                    KeyCode::Char('h') => {
                        show_all = !show_all;
                        table_state.select_first();
                        table_state.select_first_column();
                    }
                    KeyCode::Down => table_state.select_next(),
                    KeyCode::Up => table_state.select_previous(),
                    KeyCode::Right => table_state.select_next_column(),
                    KeyCode::Left => table_state.select_previous_column(),
                    KeyCode::Enter => expand = !expand,
                    _ => {}
                }
            }
        }
    }
}

impl Default for App {
    fn default() -> Self {
        let mut focus = FocusManager::new();
        focus.register(FocusTarget::Explore);
        focus.register(FocusTarget::Scan);
        focus.register(FocusTarget::Exit);
        Self {
            exit: false,
            focus,
            explore_btn: ButtonState::enabled(),
            scan_btn: ButtonState::enabled(),
            exit_btn: ButtonState::enabled(),
            selected_path: None,
            throbber_state: throbber_widgets_tui::ThrobberState::default(),
            results: Vec::new(),
        }
    }
}

fn main() -> io::Result<()> {
    ratatui::run(|terminal| App::default().run(terminal))
}
