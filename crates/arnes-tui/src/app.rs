// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

use std::io;

use arnes_core::{EventKind, SessionEvent, ToolCallOutcome};
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
    widgets::{Block, Borders, Paragraph, Wrap},
};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::frontend::UiCommand;

enum TranscriptItem {
    User(String),
    Assistant {
        thinking: String,
        text: String,
    },
    Error(String),
    ToolCall {
        name: String,
        args: String,
        outcome: Option<RenderedOutcome>,
    },
}

enum RenderedOutcome {
    Ok(String),
    Err(String),
    Denied,
    Unknown,
}

pub struct AppState {
    items: Vec<TranscriptItem>,
    streaming: String,
    streaming_thinking: String,
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
            streaming_thinking: String::new(),
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
            EventKind::ThinkingDelta { text } => {
                self.streaming_thinking.push_str(&text);
            }
            EventKind::TurnEnd { .. } => {
                self.flush_streaming();
                self.is_running = false;
                self.current_cancel = None;
            }
            EventKind::Error { message } => {
                self.streaming.clear();
                self.streaming_thinking.clear();
                self.items.push(TranscriptItem::Error(message));
                self.is_running = false;
                self.current_cancel = None;
            }
            EventKind::ToolCallStarted { name, args } => {
                self.flush_streaming();
                self.items.push(TranscriptItem::ToolCall {
                    name,
                    args: summarize_json(&args),
                    outcome: None,
                });
            }
            EventKind::ToolCallFinished { name, outcome } => {
                let rendered = render_outcome(outcome);
                let matched = self.items.iter_mut().rev().find_map(|item| match item {
                    TranscriptItem::ToolCall {
                        name: n,
                        outcome: o @ None,
                        ..
                    } if *n == name => Some(o),
                    _ => None,
                });
                if let Some(slot) = matched {
                    *slot = Some(rendered);
                } else {
                    self.items.push(TranscriptItem::ToolCall {
                        name,
                        args: String::new(),
                        outcome: Some(rendered),
                    });
                }
            }
        }
    }

    fn flush_streaming(&mut self) {
        if !self.streaming.is_empty() || !self.streaming_thinking.is_empty() {
            let thinking = std::mem::take(&mut self.streaming_thinking);
            let text = std::mem::take(&mut self.streaming);
            self.items
                .push(TranscriptItem::Assistant { thinking, text });
        }
    }
}

const MAX_INLINE_JSON: usize = 80;

fn summarize_json(value: &serde_json::Value) -> String {
    let s = serde_json::to_string(value).unwrap_or_default();
    truncate(&s, MAX_INLINE_JSON)
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}

fn render_tool_call<'a>(
    name: &'a str,
    args: &'a str,
    outcome: Option<&'a RenderedOutcome>,
) -> Vec<Line<'a>> {
    let mut lines = Vec::with_capacity(2);
    let header_style = Style::default()
        .fg(Color::Yellow)
        .add_modifier(Modifier::BOLD);
    let dim = Style::default().fg(Color::DarkGray);

    let mut header = vec![
        Span::styled("⚙ ", header_style),
        Span::styled(name.to_owned(), header_style),
    ];
    if !args.is_empty() {
        header.push(Span::styled(" ", dim));
        header.push(Span::styled(args.to_owned(), dim));
    }
    lines.push(Line::from(header));

    let result_line = match outcome {
        None => Line::from(Span::styled("  ↳ running…", dim)),
        Some(RenderedOutcome::Ok(value)) => Line::from(vec![
            Span::styled("  ↳ ", Style::default().fg(Color::Green)),
            Span::styled(value.clone(), Style::default().fg(Color::Green)),
        ]),
        Some(RenderedOutcome::Err(msg)) => Line::from(vec![
            Span::styled("  ↳ error: ", Style::default().fg(Color::Red)),
            Span::styled(msg.clone(), Style::default().fg(Color::Red)),
        ]),
        Some(RenderedOutcome::Denied) => {
            Line::from(Span::styled("  ↳ denied", Style::default().fg(Color::Red)))
        }
        Some(RenderedOutcome::Unknown) => Line::from(Span::styled(
            "  ↳ unknown tool",
            Style::default().fg(Color::Red),
        )),
    };
    lines.push(result_line);
    lines.push(Line::from(""));
    lines
}

fn render_outcome(outcome: ToolCallOutcome) -> RenderedOutcome {
    match outcome {
        ToolCallOutcome::Ok(v) => {
            if let Some(err) = v.get("error").and_then(|e| e.as_str()) {
                RenderedOutcome::Err(truncate(err, MAX_INLINE_JSON))
            } else {
                RenderedOutcome::Ok(summarize_json(&v))
            }
        }
        ToolCallOutcome::Err(msg) => RenderedOutcome::Err(truncate(&msg, MAX_INLINE_JSON)),
        ToolCallOutcome::Denied => RenderedOutcome::Denied,
        ToolCallOutcome::Unknown => RenderedOutcome::Unknown,
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
        match item {
            TranscriptItem::User(text) => {
                lines.push(Line::from(vec![
                    Span::styled(
                        "> ",
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(text.clone(), Style::default().fg(Color::Cyan)),
                ]));
            }
            TranscriptItem::Assistant { thinking, text } => {
                if !thinking.is_empty() {
                    let dim_italic = Style::default()
                        .fg(Color::DarkGray)
                        .add_modifier(Modifier::ITALIC);
                    lines.push(Line::from(Span::styled("◆ thinking", dim_italic)));
                    for line in thinking.lines() {
                        lines.push(Line::from(Span::styled(format!("  {line}"), dim_italic)));
                    }
                }
                for line in text.lines() {
                    lines.push(Line::from(Span::raw(line.to_owned())));
                }
                lines.push(Line::from(""));
            }
            TranscriptItem::Error(text) => {
                lines.push(Line::from(Span::styled(
                    format!("error: {}", text),
                    Style::default().fg(Color::Red),
                )));
            }
            TranscriptItem::ToolCall {
                name,
                args,
                outcome,
            } => {
                lines.extend(render_tool_call(name, args, outcome.as_ref()));
            }
        }
    }
    if !state.streaming_thinking.is_empty() {
        let dim_italic = Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::ITALIC);
        lines.push(Line::from(Span::styled("◆ thinking", dim_italic)));
        for line in state.streaming_thinking.lines() {
            lines.push(Line::from(Span::styled(format!("  {line}"), dim_italic)));
        }
        if state.streaming.is_empty() {
            lines.push(Line::from(Span::styled(
                "▋",
                Style::default().fg(Color::DarkGray),
            )));
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

    let transcript = Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false });
    let transcript_height = chunks[0].height as usize;
    let wrapped_lines = transcript.line_count(chunks[0].width);
    let max_scroll = wrapped_lines.saturating_sub(transcript_height);
    let max_scroll_u16 = max_scroll.min(u16::MAX as usize) as u16;
    state.last_transcript_height = chunks[0].height;
    state.last_max_scroll = max_scroll_u16;
    if state.scroll_offset > max_scroll_u16 {
        state.scroll_offset = max_scroll_u16;
    }
    let scroll = max_scroll_u16.saturating_sub(state.scroll_offset);
    f.render_widget(transcript.scroll((scroll, 0)), chunks[0]);

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
                                    state.items.push(TranscriptItem::User(text.clone()));
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
