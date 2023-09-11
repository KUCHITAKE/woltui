mod config;
mod statefullist;
mod states;

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Span, Spans},
    widgets::*,
    Frame, Terminal,
};
use regex::Regex;
use sm::{AsEnum, Initializer, Transition};
use std::{
    io::{self, ErrorKind},
    str::FromStr,
    time::{Duration, Instant},
};
use tui_textarea::TextArea;

use crate::app::{config::*, statefullist::StatefulList, states::*};

pub struct App<'a> {
    pub state_machine: Variant,
    pub machines: StatefulList<(String, String)>,
    pub status_message: String,
    pub textarea: TextArea<'a>,
    pub editing_name: String,
    pub editing_mac: String,
    pub mac_regex: Regex,
    pub popup_time: Option<Instant>,
}

impl<'a> App<'a> {
    pub fn new() -> App<'a> {
        let sm = states::Machine::new(Main).as_enum();
        let mut app = App {
            state_machine: sm,
            machines: StatefulList::with_items(vec![]),
            status_message: "".into(),
            textarea: TextArea::default(),
            editing_name: "".into(),
            editing_mac: "".into(),
            mac_regex: Regex::new(r"^([0-9A-Fa-f]{2}[:-]){5}([0-9A-Fa-f]{2})$").unwrap(),
            popup_time: None,
        };
        app.load_machines().expect("Failed to load machines");
        app
    }

    pub fn load_machines(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let config_path = match dirs::home_dir() {
            Some(home_dir) => Ok(home_dir.join(".wol").join("config")),
            None => Err(Box::new(io::Error::new(
                ErrorKind::NotFound,
                "Home directory not found",
            ))),
        }?;

        let config = config::read_config(config_path.as_path())?.machines;

        let machine_tuples: Vec<(String, String)> = config
            .iter()
            .map(|m| (m.name.clone(), m.mac_address.clone()))
            .collect();

        self.machines = StatefulList::with_items(machine_tuples);

        Ok(())
    }

    pub fn add_machine(&mut self, name: &str, mac: &str) -> Result<(), Box<dyn std::error::Error>> {
        self.machines
            .items
            .push((name.to_string(), mac.to_string()));

        self.save_machines()?;

        Ok(())
    }

    pub fn delete_machine(&mut self, index: usize) -> Result<(), Box<dyn std::error::Error>> {
        self.machines.items.remove(index);

        self.save_machines()?;

        Ok(())
    }

    pub fn save_machines(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let config_path = match dirs::home_dir() {
            Some(home_dir) => Ok(home_dir.join(".wol").join("config")),
            None => Err(Box::new(io::Error::new(
                ErrorKind::NotFound,
                "Home directory not found",
            ))),
        }?;

        let config = config::Config {
            machines: self
                .machines
                .items
                .iter()
                .map(|(name, mac_address)| config::Machine {
                    name: name.clone(),
                    mac_address: mac_address.clone(),
                })
                .collect(),
        };

        write_config(config_path.as_path(), &config)?;

        Ok(())
    }

    fn on_tick(&mut self) {
        if let SendPopBySend(m) = &self.state_machine {
            if let Some(popup_time) = self.popup_time {
                if popup_time.elapsed() > Duration::from_secs(1) {
                    self.popup_time = None;
                    self.state_machine = m.clone().transition(Next).as_enum();
                }
            }
        }
    }
}

pub fn run_app<B: Backend>(
    terminal: &mut Terminal<B>,
    mut app: App,
    tick_rate: Duration,
) -> io::Result<()> {
    let mut last_tick = Instant::now();
    loop {
        terminal.draw(|f| ui(f, &mut app))?;

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));
        if crossterm::event::poll(timeout)? {
            if let Event::Key(key) = event::read()?.into() {
                if key.kind == KeyEventKind::Press {
                    let mut new_machine = None;
                    let mut delete_machine = None;
                    if let Some(state) = match (key, &mut app.state_machine) {
                        (input, NameInputByAdd(m)) => match input.code {
                            KeyCode::Enter => {
                                app.editing_name = app.textarea.lines()[0].clone();
                                app.textarea = TextArea::default();
                                Some(m.clone().transition(Next).as_enum())
                            }
                            KeyCode::Esc => {
                                app.textarea = TextArea::default();
                                Some(m.clone().transition(Cancel).as_enum())
                            }
                            _ => {
                                app.textarea.input(input);
                                None
                            }
                        },
                        (input, MacInputByNext(m)) => match input.code {
                            KeyCode::Enter => {
                                app.editing_mac = app.textarea.lines()[0].clone();
                                if app.mac_regex.is_match(&app.editing_mac) {
                                    app.textarea = TextArea::default();
                                    Some(m.clone().transition(Next).as_enum())
                                } else {
                                    None
                                }
                            }
                            KeyCode::Esc => {
                                app.textarea = TextArea::default();
                                Some(m.clone().transition(Cancel).as_enum())
                            }
                            _ => {
                                app.textarea.input(input);
                                None
                            }
                        },
                        (input, state) => match (input.code, state) {
                            (KeyCode::Char('q'), InitialMain(_))
                            | (KeyCode::Char('q'), MainByCancel(_))
                            | (KeyCode::Char('q'), MainByNext(_)) => return Ok(()),
                            (KeyCode::Down, InitialMain(_))
                            | (KeyCode::Down, MainByCancel(_))
                            | (KeyCode::Down, MainByNext(_)) => {
                                app.machines.next();
                                None
                            }
                            (KeyCode::Up, InitialMain(_))
                            | (KeyCode::Up, MainByCancel(_))
                            | (KeyCode::Up, MainByNext(_)) => {
                                app.machines.previous();
                                None
                            }
                            (KeyCode::Enter, InitialMain(m)) => {
                                if let Some(selected) = app.machines.state.selected() {
                                    let (_, mac) = app.machines.items[selected].clone();
                                    wol::send_wol(
                                        wol::MacAddr::from_str(mac.as_str()).unwrap(),
                                        None,
                                        None,
                                    )
                                    .unwrap();
                                    app.popup_time = Some(Instant::now());
                                    Some(m.clone().transition(Send).as_enum())
                                } else {
                                    None
                                }
                            }
                            (KeyCode::Enter, MainByNext(m)) => {
                                if let Some(selected) = app.machines.state.selected() {
                                    let (_, mac) = app.machines.items[selected].clone();
                                    wol::send_wol(
                                        wol::MacAddr::from_str(mac.as_str()).unwrap(),
                                        None,
                                        None,
                                    )
                                    .unwrap();
                                    app.popup_time = Some(Instant::now());
                                    Some(m.clone().transition(Send).as_enum())
                                } else {
                                    None
                                }
                            }
                            (KeyCode::Enter, MainByCancel(m)) => {
                                if let Some(selected) = app.machines.state.selected() {
                                    let (_, mac) = app.machines.items[selected].clone();
                                    wol::send_wol(
                                        wol::MacAddr::from_str(mac.as_str()).unwrap(),
                                        None,
                                        None,
                                    )
                                    .unwrap();
                                    app.popup_time = Some(Instant::now());
                                    Some(m.clone().transition(Send).as_enum())
                                } else {
                                    None
                                }
                            }
                            (KeyCode::Char('a'), InitialMain(m)) => {
                                Some(m.clone().transition(Add).as_enum())
                            }
                            (KeyCode::Char('a'), MainByNext(m)) => {
                                Some(m.clone().transition(Add).as_enum())
                            }
                            (KeyCode::Char('a'), MainByCancel(m)) => {
                                Some(m.clone().transition(Add).as_enum())
                            }
                            (KeyCode::Char('d'), InitialMain(m)) => {
                                if app.machines.state.selected().is_some() {
                                    Some(m.clone().transition(Delete).as_enum())
                                } else {
                                    None
                                }
                            }
                            (KeyCode::Char('d'), MainByCancel(m)) => {
                                if app.machines.state.selected().is_some() {
                                    Some(m.clone().transition(Delete).as_enum())
                                } else {
                                    None
                                }
                            }
                            (KeyCode::Char('d'), MainByNext(m)) => {
                                if app.machines.state.selected().is_some() {
                                    Some(m.clone().transition(Delete).as_enum())
                                } else {
                                    None
                                }
                            }
                            (KeyCode::Char('Y'), ConfirmAddByNext(m)) => {
                                new_machine =
                                    Some((app.editing_name.clone(), app.editing_mac.clone()));
                                Some(m.clone().transition(Next).as_enum())
                            }
                            (KeyCode::Char('n'), ConfirmAddByNext(m))
                            | (KeyCode::Char('N'), ConfirmAddByNext(m))
                            | (KeyCode::Esc, ConfirmAddByNext(m)) => {
                                Some(m.clone().transition(Cancel).as_enum())
                            }
                            (KeyCode::Char('Y'), ConfirmDeleteByDelete(m)) => {
                                delete_machine = Some(app.machines.state.selected().unwrap());
                                app.machines.previous();
                                Some(m.clone().transition(Next).as_enum())
                            }
                            (KeyCode::Char('n'), ConfirmDeleteByDelete(m))
                            | (KeyCode::Char('N'), ConfirmDeleteByDelete(m))
                            | (KeyCode::Esc, ConfirmDeleteByDelete(m)) => {
                                Some(m.clone().transition(Cancel).as_enum())
                            }
                            _ => None,
                        },
                    } {
                        app.state_machine = state;
                    }
                    if let Some((name, mac)) = new_machine {
                        app.add_machine(name.as_str(), mac.as_str())
                            .expect("cannot add machine");
                    }
                    if let Some(index) = delete_machine {
                        app.delete_machine(index).expect("can not delete machine");
                    }
                }
            }
        }
        if last_tick.elapsed() >= tick_rate {
            app.on_tick();
            last_tick = Instant::now();
        }
    }
}

fn ui<B: Backend>(f: &mut Frame<B>, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Max(2)].as_ref())
        .split(f.size());

    let items: Vec<ListItem> = app
        .machines
        .items
        .iter()
        .map(|(name, mac)| {
            let lines = Spans::from(vec![
                Span::from(format!("{:<20}", name)),
                Span::from(mac.as_str()),
            ]);
            ListItem::new(lines).style(Style::default().fg(Color::Black).bg(Color::White))
        })
        .collect();

    let items = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Machines"))
        .highlight_style(
            Style::default()
                .bg(Color::LightGreen)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(">> ");

    // We can now render the item list
    f.render_stateful_widget(items, chunks[0], &mut app.machines.state);

    let status_line = Spans::from(Span::raw(&app.status_message));
    f.render_widget(
        Paragraph::new(status_line).block(Block::default().borders(Borders::TOP)),
        chunks[1],
    );

    match &app.state_machine {
        NameInputByAdd(_) => {
            app.textarea
                .set_block(Block::default().borders(Borders::ALL).title("Machine Name"));
            let widget = app.textarea.widget();
            let area = centered_rect(60, 3, f.size());
            f.render_widget(Clear, area);
            f.render_widget(widget, area);
        }
        MacInputByNext(_) => {
            let style = if app.mac_regex.is_match(app.textarea.lines()[0].as_str()) {
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Red)
            };
            app.textarea.set_block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("MAC Address")
                    .style(style),
            );
            let widget = app.textarea.widget();
            let area = centered_rect(60, 3, f.size());
            f.render_widget(Clear, area);
            f.render_widget(widget, area);
        }
        ConfirmAddByNext(_) => {
            let block = Block::default().borders(Borders::ALL);
            let text = format!(
                "name: {}\nMAC:  {}\n\nAdd new machine? (Y/n)",
                app.editing_name, app.editing_mac
            );
            let paragraph = Paragraph::new(text);
            let area = centered_rect(60, 6, f.size());
            f.render_widget(Clear, area);
            f.render_widget(paragraph.block(block), area);
        }
        ConfirmDeleteByDelete(_) => {
            let block = Block::default().borders(Borders::ALL);
            let selected = app.machines.state.selected().unwrap_or_default();
            let text = format!(
                "name: {}\nMAC:  {}\n\nDelete machine? (Y/n)",
                app.machines.items[selected].0, app.machines.items[selected].1
            );
            let paragraph = Paragraph::new(text);
            let area = centered_rect(60, 6, f.size());
            f.render_widget(Clear, area);
            f.render_widget(paragraph.block(block), area);
        }
        SendPopBySend(_) => {
            let style = Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD);
            let block = Block::default().borders(Borders::ALL).style(style);
            let selected = app.machines.state.selected().unwrap_or_default();
            let text = format!(
                "name: {}\nMAC:  {}\n\nSent wol packet!",
                app.machines.items[selected].0, app.machines.items[selected].1
            );
            let paragraph = Paragraph::new(text);
            let area = centered_rect(60, 6, f.size());
            f.render_widget(Clear, area);
            f.render_widget(paragraph.block(block), area);
        }
        _ => {}
    };
}

fn centered_rect(percent_x: u16, y_line: u16, r: Rect) -> Rect {
    let vertical_padding = (r.height - 3) / 2;

    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Length(vertical_padding),
                Constraint::Length(y_line),
                Constraint::Length(vertical_padding),
            ]
            .as_ref(),
        )
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints(
            [
                Constraint::Percentage((100 - percent_x) / 2),
                Constraint::Percentage(percent_x),
                Constraint::Percentage((100 - percent_x) / 2),
            ]
            .as_ref(),
        )
        .split(popup_layout[1])[1]
}
