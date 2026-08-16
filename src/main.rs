use std::io;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, read};
use ratatui_interact::components::{Button, ButtonState, ButtonVariant};
use ratatui_interact::state::FocusManager;
use ratatui::{crossterm, widgets::FrameExt};
use ratatui::{
    buffer::Buffer,
    layout::{Rect, Direction, Constraint, Flex, Layout},
    style::{Stylize, Color, Style},
    symbols::border,
    text::Line,
    widgets::{Block, Paragraph, Widget, Borders, Padding, Table, Row, TableState},
    DefaultTerminal, Frame,
};
use ratatui_explorer::{FileExplorerBuilder, Theme};
use akhtabooti_core::{search_directory, search_file};
use std::sync::mpsc;
use std::thread;
use std::path::Path;
use std::time::Duration;

fn center(area: Rect, width: Constraint, height: Constraint) -> Rect {
    let [area] = Layout::horizontal([width]).flex(Flex::Center).areas(area);
    let [area] = Layout::vertical([height]).flex(Flex::Center).areas(area);
    area
}

fn center_horizontal(area: Rect, width: Constraint) -> Rect {
    let [area] = Layout::horizontal([width]).flex(Flex::Center).areas(area);
    area
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
    results: Vec<akhtabooti_core::FilePIIs>,
}

impl App {
    pub fn run(&mut self, terminal: &mut DefaultTerminal) -> io::Result<()> {
        self.sync_button_focus();
        self.main_menu(terminal)?;
        Ok(())
    }

    fn draw(&self, frame: &mut Frame) {
        frame.render_widget(self, frame.area());
    }

    fn sync_button_focus(&mut self) {
        self.explore_btn.focused = self.focus.is_focused(&FocusTarget::Explore);
        self.scan_btn.focused = self.focus.is_focused(&FocusTarget::Scan);
        self.exit_btn.focused = self.focus.is_focused(&FocusTarget::Exit);
    }

    fn exit(&mut self) {
        self.exit = true;
    }

    fn main_menu(&mut self, terminal: &mut DefaultTerminal) -> Result<(), io::Error> {
        while !self.exit {
            self.sync_button_focus();
            terminal.draw(|frame| {
                let title = Line::from(" akhtabooti ".bold());
                let instructions = Line::from(vec![
                    " (↑) ".bold(), "prev |".into(),
                    " (↓) ".bold(), "next |".into(),
                    " (Enter) ".bold(), "select |".into(),
                    " (q) ".bold(), "quit ".into(),
                ]);
                let block = Block::bordered()
                    .title(title.centered())
                    .title_bottom(instructions.centered())
                    .border_set(border::THICK);

                let inner = block.inner(frame.area());
                frame.render_widget(block, frame.area());

                let explore_label = match &self.selected_path {
                    Some(path) => path.to_string(),
                    None => "[ Select path to scan ]".to_string(),
                };

                let explore_button = Button::new(
                    &explore_label, 
                    &self.explore_btn
                ).variant(ButtonVariant::SingleLine);
                let scan_button = Button::new(
                    "[ Scan ]", 
                    &self.scan_btn
                ).variant(ButtonVariant::SingleLine);
                let exit_button = Button::new(
                    "[ Exit ]", 
                    &self.exit_btn
                ).variant(ButtonVariant::SingleLine);

                let explore_w = explore_button.min_width();
                let scan_w = scan_button.min_width();
                let exit_w = exit_button.min_width();

                let show_scan = self.selected_path.is_some();
 
                let content = if show_scan {
                    center(inner, Constraint::Fill(1), Constraint::Length(5))
                } else {
                    center(inner, Constraint::Fill(1), Constraint::Length(3))
                };
 
                if show_scan {
                    let rows = Layout::vertical([
                        Constraint::Length(1),
                        Constraint::Length(1),
                        Constraint::Length(1),
                        Constraint::Length(1),
                        Constraint::Length(1),
                    ])
                    .split(content);
 
                    let explore_area = center_horizontal(rows[0], Constraint::Length(explore_w));
                    frame.render_widget(explore_button, explore_area);
 
                    let scan_area = center_horizontal(rows[2], Constraint::Length(scan_w));
                    frame.render_widget(scan_button, scan_area);
 
                    let exit_area = center_horizontal(rows[4], Constraint::Length(exit_w));
                    frame.render_widget(exit_button, exit_area);
                } else {
                    let rows = Layout::vertical([
                        Constraint::Length(1),
                        Constraint::Length(1),
                        Constraint::Length(1),
                    ])
                    .split(content);
 
                    let explore_area = center_horizontal(rows[0], Constraint::Length(explore_w));
                    frame.render_widget(explore_button, explore_area);
 
                    let exit_area = center_horizontal(rows[2], Constraint::Length(exit_w));
                    frame.render_widget(exit_button, exit_area);
                }
            });

            let event = read()?;
            if let Event::Key(key) = &event {
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
                            self.exit()
                        } else if self.focus.is_focused(&FocusTarget::Scan) {
                            if let Some(path) = &self.selected_path {
                                self.scanning(terminal, path.to_string())?;
                                self.scan_results(terminal)?;
                            }
                        } else if self.focus.is_focused(&FocusTarget::Explore) {
                            let path = self.open_file_explorer(terminal)?;
                            if !path.is_empty() {
                                self.selected_path = Some(path);
                            }
                        }
                    }
                    _ => {}
                }
            }
        };
        Ok(())
    }

    fn open_file_explorer(&mut self, terminal: &mut DefaultTerminal) -> Result<String, io::Error> {
        let mut selected = "";
        let theme = Theme::default().add_default_title();
        let mut file_explorer = FileExplorerBuilder::build_with_theme(theme)?;

        loop {
            terminal.draw(|frame| {
                let title = Line::from(" akhtabooti ".bold());
                let instructions = Line::from(vec![
                    " (q) ".bold(), "quit |".into(),
                    " (enter) ".bold(), "enter |".into(),
                    " (space) ".bold(), "select |".into(),
                ]);

                let block = Block::bordered()
                    .title(title.centered())
                    .title_bottom(instructions.centered())
                    .border_set(border::THICK)
                    .padding(Padding::new(1, 1, 0, 0));

                let inner = block.inner(frame.area());
                frame.render_widget(block, frame.area());

                frame.render_widget_ref(file_explorer.widget(), inner);
            })?;

            let event = read()?;
            if let Event::Key(key) = &event {
                match key.code {
                    KeyCode::Char('q') => break,
                    KeyCode::Char(' ') => {
                        let current = file_explorer.current();
                        selected = current.path.as_path().to_str().unwrap();
                        break;
                    }
                    _ => {}
                }
            }
            file_explorer.handle(&event)?;
        };
        Ok(selected.to_string())
    }

    fn on_tick(&mut self) {
        self.throbber_state.calc_next();
    }

    fn scan_path(path: String) -> Vec<akhtabooti_core::FilePIIs> {
        if Path::new(&path).is_dir() {
            return search_directory(&path).unwrap();
        } else {
            return vec![search_file(&path).unwrap()];
        }
    }

    fn scanning(&mut self, terminal: &mut DefaultTerminal, path: String) -> Result<(), io::Error> {
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
                let title = Line::from(" akhtabooti ".bold());

                let block = Block::bordered()
                    .title(title.centered())
                    .border_set(border::THICK);

                let inner = block.inner(frame.area());
                frame.render_widget(block, frame.area());

                let label = "Scanning...";
                let width = label.chars().count() as u16 + 4;

                let area = center(inner, Constraint::Length(width), Constraint::Length(1));

                let full = throbber_widgets_tui::Throbber::default()
                    .label("Running...")
                    .style(ratatui::style::Style::default().fg(ratatui::style::Color::Cyan))
                    .throbber_style(ratatui::style::Style::default().fg(ratatui::style::Color::Red).add_modifier(ratatui::style::Modifier::BOLD))
                    .throbber_set(throbber_widgets_tui::BLACK_CIRCLE)
                    .use_type(throbber_widgets_tui::WhichUse::Spin);
                frame.render_stateful_widget(full, area, &mut self.throbber_state);
            })?;

            thread::sleep(Duration::from_millis(80));
        }

        Ok(())
    }
    
    fn scan_results(&mut self, terminal: &mut DefaultTerminal) -> Result<(), io::Error> {
        let mut table_state = TableState::default();
        table_state.select_first();
        table_state.select_first_column();
        loop {
            terminal.draw(|frame| {
                let title = Line::from(" akhtabooti ".bold());
                let instructions = Line::from(vec![
                    " (↑) ".bold(), "prev |".into(),
                    " (↓) ".bold(), "next |".into(),
                    " (Enter) ".bold(), "expand |".into(),
                    " (q) ".bold(), "quit ".into(),
                ]);

                let block = Block::bordered()
                    .title(title.centered())
                    .title_bottom(instructions.centered())
                    .border_set(border::THICK);

                let inner = block.inner(frame.area());
                frame.render_widget(block, frame.area());
                
                let header = Row::new(["filename","email_accounts", "phone_numbers", "other_piis"])
                .style(Style::new().bold())
                .bottom_margin(1);

                let mut rows = Vec::new();

                for pii in &mut *self.results {
                    let row = Row::new([pii.filename.clone(), pii.email_accounts.len().to_string(), pii.phone_numbers.len().to_string(), pii.other_piis.len().to_string()]);
                    rows.push(row);
                }

                let widths = [
                    Constraint::Percentage(70),
                    Constraint::Percentage(10),
                    Constraint::Percentage(10),
                    Constraint::Percentage(10),
                ];
                let table = Table::new(rows, widths)
                    .header(header)
                    .column_spacing(1)
                    .style(Color::White)
                    .row_highlight_style(Style::new().on_black().bold())
                    .column_highlight_style(Color::Gray)
                    .cell_highlight_style(Style::new().reversed().yellow())
                    .highlight_symbol("> ");

                frame.render_stateful_widget(table, inner, &mut table_state);
                        //
                        // let color = if i == self.selected_scan_result { Color::Yellow } else { Color::Green };
                        // let title = Line::from(vec!["filename: ".bold(), pii.filename.clone().into()]);
                        // frame.render_widget(
                        //     Paragraph::new(vec![
                        //         Line::from(vec!["email_accounts: ".bold(), pii.email_accounts.len().to_string().into()]),
                        //         Line::from(vec!["phone_numbers: ".bold(), pii.phone_numbers.len().to_string().into()]),
                        //         Line::from(vec!["other_piis: ".bold(), pii.other_piis.len().to_string().into()]),
                        // ])
                        // .block(Block::new().fg(color).borders(Borders::ALL).title(title)),
                        // cell
                        // );
            })?;
        
            let event = read()?;
            if let Event::Key(key) = &event {
                match key.code {
                    KeyCode::Char('q') => return Ok(()),
                    KeyCode::Down => table_state.select_next(),
                    KeyCode::Up => table_state.select_previous(),
                    KeyCode::Right => table_state.select_next_column(),
                    KeyCode::Left => table_state.select_previous_column(),
                    KeyCode::Enter => {
                        self.expand_result(terminal, table_state.selected().unwrap())?;
                    },
                    _ => {}
                }
            }
        };
        Ok(())
    }
    
    fn expand_result(&mut self, terminal: &mut DefaultTerminal, index: usize) -> Result<(), io::Error> {
        loop {
            terminal.draw(|frame| {
                let title = Line::from(" akhtabooti ".bold());
                let instructions = Line::from(vec![
                    " (q) ".bold(), "quit |".into(),
                ]);

                let block = Block::bordered()
                    .title(title.centered())
                    .title_bottom(instructions.centered())
                    .border_set(border::THICK);

                let inner = block.inner(frame.area());
                frame.render_widget(block, frame.area());

                let layout = Layout::default()
                    .direction(Direction::Horizontal)
                    .margin(1)
                    .constraints(vec![
                        Constraint::Fill(1)
                    ])
                    .split(inner);
                
                let pii = &self.results[index];
                let max_lines = 8;
                        
                let title = Line::from(vec!["filename: ".bold(), pii.filename.clone().into()]);
                let mut email_accounts: Vec<Line> = pii.email_accounts.iter()
                    .map(|s| Line::from(format!("- {s}")))
                    .collect();
                email_accounts.truncate(max_lines);
                let mut phone_numbers: Vec<Line> = pii.phone_numbers.iter()
                    .map(|s| Line::from(format!("- {s}")))
                    .collect();
                phone_numbers.truncate(max_lines);
                let mut other_piis: Vec<Line> = pii.other_piis.iter()
                    .map(|s| Line::from(format!("- {s}")))
                    .collect();
                other_piis.truncate(max_lines);

                let results: Vec<Line> = [
                    vec![Line::from("email_accounts: ".bold())],
                    email_accounts,
                    vec![Line::from("")],
                    vec![Line::from("phone_numbers: ".bold())],
                    phone_numbers,
                    vec![Line::from("")],
                    vec![Line::from("other_piis: ".bold())],
                    other_piis,
                ]
                .into_iter()
                .flatten()
                .collect();

                frame.render_widget(
                    Paragraph::new(results)
                        .block(Block::new().bold().fg(Color::Yellow).borders(Borders::ALL).title(title)),
                        layout[0]
                )
            })?;
            let event = read()?;
            if let Event::Key(key) = &event {
                match key.code {
                    KeyCode::Char('q') => break,
                    _ => {}
                }
            }
        }

        Ok(())
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

impl Widget for &App {
    fn render(self, area: Rect, buf: &mut Buffer) {
    }
//         let title = Line::from(" akhtabooti ".bold());
//         let instructions = Line::from(vec![
//             " (↑) ".bold(), "prev |".into(),
//             " (↓) ".bold(), "next |".into(),
//             " (Enter) ".bold(), "select |".into(),
//             " (q) ".bold(), "quit ".into(),
//         ]);
//         let block = Block::bordered()
//             .title(title.centered())
//             .title_bottom(instructions.centered())
//             .border_set(border::THICK);
//
//         let inner = block.inner(area);
//         block.render(area, buf);
//
//         let explore_label = match &self.selected_path {
//             Some(path) => path.to_string(),
//             None => "[ Select path to scan ]".to_string(),
//         };
//
//         let explore_button = Button::new(
//             &explore_label, 
//             &self.explore_btn
//         ).variant(ButtonVariant::SingleLine);
//         let scan_button = Button::new(
//             "[ Scan ]", 
//             &self.scan_btn
//         ).variant(ButtonVariant::SingleLine);
//         let exit_button = Button::new(
//             "[ Exit ]", 
//             &self.exit_btn
//         ).variant(ButtonVariant::SingleLine);
//
//         let explore_w = explore_button.min_width();
//         let scan_w = scan_button.min_width();
//         let exit_w = exit_button.min_width();
//
//         let content = center(inner, Constraint::Fill(1), Constraint::Length(5));
//
//         let rows = Layout::vertical([
//             Constraint::Length(1),
//             Constraint::Length(1),
//             Constraint::Length(1),
//             Constraint::Length(1),
//             Constraint::Length(1),
//         ])
//         .split(content);
//
//         let explore_area = center_horizontal(rows[0], Constraint::Length(explore_w));
//         explore_button.render_stateful(explore_area, buf);
//
//         let scan_area = center_horizontal(rows[2], Constraint::Length(scan_w));
//         scan_button.render_stateful(scan_area, buf);
//
//         let exit_area = center_horizontal(rows[4], Constraint::Length(exit_w));
//         exit_button.render_stateful(exit_area, buf);
//     }
}

fn main() -> io::Result<()> {
    ratatui::run(|terminal| App::default().run(terminal))
}
