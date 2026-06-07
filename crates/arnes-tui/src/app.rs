// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

use std::io;

use arnes_core::{EventKind, SessionEvent};
use crossterm::event::{
    Event, EventStream, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEvent, MouseEventKind,
};
use futures_util::StreamExt;
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph},
};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::frontend::UiCommand;

enum ItemRole {
    User,
    Assistant,
    Error,
}

struct TranscriptItem {
    role: ItemRole,
    text: String,
}

pub struct AppState {
    items: Vec<TranscriptItem>,
    streaming: String,
    input: String,
    is_running: bool,
    model_name: String,
    current_cancel: Option<CancellationToken>,
    // Lines scrolled up from the bottom of the transcript. 0 follows the tail.
    scroll_offset: u16,
    last_transcript_height: u16,
    last_max_scroll: u16,
}

impl AppState {
    pub fn new(model_name: String) -> Self {
        Self {
            items: Vec::new(),
            streaming: String::new(),
            input: String::new(),
            is_running: false,
            model_name,
            current_cancel: None,
            scroll_offset: 0,
            last_transcript_height: 0,
            last_max_scroll: 0,
        }
    }

    fn scroll_up(&mut self, by: u16) {
        self.scroll_offset = self
            .scroll_offset
            .saturating_add(by)
            .min(self.last_max_scroll);
    }

    fn scroll_down(&mut self, by: u16) {
        self.scroll_offset = self.scroll_offset.saturating_sub(by);
    }

    fn scroll_to_top(&mut self) {
        self.scroll_offset = self.last_max_scroll;
    }

    fn scroll_to_bottom(&mut self) {
        self.scroll_offset = 0;
    }

    fn page_size(&self) -> u16 {
        self.last_transcript_height.saturating_sub(1).max(1)
    }

    fn handle_session_event(&mut self, ev: SessionEvent) {
        match ev.kind {
            EventKind::TurnStart => {
                self.is_running = true;
            }
            EventKind::TextDelta { text } => {
                self.streaming.push_str(&text);
            }
            EventKind::ThinkingDelta { .. } => {}
            EventKind::TurnEnd { .. } => {
                if !self.streaming.is_empty() {
                    let text = std::mem::take(&mut self.streaming);
                    self.items.push(TranscriptItem {
                        role: ItemRole::Assistant,
                        text,
                    });
                }
                self.is_running = false;
                self.current_cancel = None;
            }
            EventKind::Error { message } => {
                self.streaming.clear();
                self.items.push(TranscriptItem {
                    role: ItemRole::Error,
                    text: message,
                });
                self.is_running = false;
                self.current_cancel = None;
            }
        }
    }
}

fn render(f: &mut Frame, state: &mut AppState) {
    let area = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(3),
        ])
        .split(area);

    // Transcript
    let mut lines: Vec<Line> = Vec::new();
    for item in &state.items {
        match item.role {
            ItemRole::User => {
                lines.push(Line::from(vec![
                    Span::styled(
                        "> ",
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(item.text.clone(), Style::default().fg(Color::Cyan)),
                ]));
            }
            ItemRole::Assistant => {
                for line in item.text.lines() {
                    lines.push(Line::from(Span::raw(line.to_owned())));
                }
                lines.push(Line::from(""));
            }
            ItemRole::Error => {
                lines.push(Line::from(Span::styled(
                    format!("error: {}", item.text),
                    Style::default().fg(Color::Red),
                )));
            }
        }
    }
    if !state.streaming.is_empty() {
        for line in state.streaming.lines() {
            lines.push(Line::from(Span::raw(line.to_owned())));
        }
        lines.push(Line::from(Span::styled(
            "▋",
            Style::default().fg(Color::Yellow),
        )));
    }

    let transcript_height = chunks[0].height as usize;
    let max_scroll = lines.len().saturating_sub(transcript_height);
    let max_scroll_u16 = max_scroll.min(u16::MAX as usize) as u16;
    state.last_transcript_height = chunks[0].height;
    state.last_max_scroll = max_scroll_u16;
    if state.scroll_offset > max_scroll_u16 {
        state.scroll_offset = max_scroll_u16;
    }
    let scroll = max_scroll_u16.saturating_sub(state.scroll_offset);
    let transcript = Paragraph::new(Text::from(lines)).scroll((scroll, 0));
    f.render_widget(transcript, chunks[0]);

    // Status line
    let status_text = if state.is_running {
        format!(" arnes  ·  {}  ·  running…", state.model_name)
    } else {
        format!(" arnes  ·  {}  ·  ctrl-c to quit", state.model_name)
    };
    let status = Paragraph::new(status_text).style(Style::default().fg(Color::DarkGray));
    f.render_widget(status, chunks[1]);

    // Input box
    let border_style = if state.is_running {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default().fg(Color::White)
    };
    let input_display = format!("> {}", state.input);
    let input_widget = Paragraph::new(input_display)
        .block(Block::default().borders(Borders::ALL).style(border_style));
    f.render_widget(input_widget, chunks[2]);

    // Cursor inside input box (hidden while running)
    if !state.is_running {
        let max_x = chunks[2].x + chunks[2].width.saturating_sub(2);
        let cursor_x = (chunks[2].x + 1 + 2 + state.input.len() as u16).min(max_x);
        let cursor_y = chunks[2].y + 1;
        f.set_cursor_position((cursor_x, cursor_y));
    }
}

pub async fn run(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    mut ui_rx: mpsc::UnboundedReceiver<UiCommand>,
    prompt_tx: mpsc::Sender<(String, CancellationToken)>,
    model_name: String,
) -> io::Result<()> {
    let mut state = AppState::new(model_name);
    let mut event_stream = EventStream::new();

    loop {
        terminal.draw(|f| render(f, &mut state))?;

        tokio::select! {
            biased;

            maybe_event = event_stream.next() => {
                let Some(Ok(event)) = maybe_event else { break; };
                match event {
                    Event::Key(KeyEvent { code, modifiers, kind: KeyEventKind::Press, .. }) => {
                        match (code, modifiers) {
                            (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                                if let Some(cancel) = &state.current_cancel {
                                    cancel.cancel();
                                }
                                break;
                            }
                            (KeyCode::Esc, _) if state.is_running => {
                                if let Some(cancel) = &state.current_cancel {
                                    cancel.cancel();
                                }
                            }
                            (KeyCode::Enter, _) if !state.is_running => {
                                let text = state.input.trim().to_string();
                                if !text.is_empty() {
                                    state.input.clear();
                                    state.items.push(TranscriptItem {
                                        role: ItemRole::User,
                                        text: text.clone(),
                                    });
                                    state.scroll_to_bottom();
                                    let cancel = CancellationToken::new();
                                    state.current_cancel = Some(cancel.clone());
                                    let _ = prompt_tx.send((text, cancel)).await;
                                }
                            }
                            (KeyCode::Backspace, _) if !state.is_running => {
                                state.input.pop();
                            }
                            (KeyCode::PageUp, _) => {
                                let page = state.page_size();
                                state.scroll_up(page);
                            }
                            (KeyCode::PageDown, _) => {
                                let page = state.page_size();
                                state.scroll_down(page);
                            }
                            (KeyCode::Home, KeyModifiers::CONTROL) => {
                                state.scroll_to_top();
                            }
                            (KeyCode::End, KeyModifiers::CONTROL) => {
                                state.scroll_to_bottom();
                            }
                            (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT)
                                if !state.is_running =>
                            {
                                state.input.push(c);
                            }
                            _ => {}
                        }
                    }
                    Event::Mouse(MouseEvent { kind, .. }) => {
                        match kind {
                            MouseEventKind::ScrollUp => state.scroll_up(3),
                            MouseEventKind::ScrollDown => state.scroll_down(3),
                            _ => {}
                        }
                    }
                    Event::Resize(_, _) => {}
                    _ => {}
                }
            }

            maybe_cmd = ui_rx.recv() => {
                if let Some(UiCommand::Event(ev)) = maybe_cmd {
                    state.handle_session_event(ev);
                }
            }
        }
    }

    Ok(())
}
