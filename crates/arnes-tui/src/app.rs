// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

use std::io;

use arnes_core::{EditTextFileProposal, EventKind, Permission, ToolCallOutcome};
use crossterm::event::{
    Event, EventStream, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEvent, MouseEventKind,
};
use futures_util::StreamExt;
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};
use similar::{ChangeTag, TextDiff};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;
use tracing::debug;

use crate::frontend::UiCommand;

enum TranscriptItem {
    User(String),
    Assistant {
        thinking: String,
        text: String,
    },
    Error(String),
    ToolCall {
        id: String,
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

// A gated tool call awaiting the user's allow/deny answer.
struct PendingPermission {
    tool_name: String,
    args: String,
    // The edited file's path and the rendered before/after diff, when the
    // proposal describes a file edit. `None` falls back to the args summary.
    path: Option<String>,
    diff: Option<Vec<Line<'static>>>,
    // Lines of the diff scrolled past the top. 0 shows the start.
    scroll: u16,
    // Height of the diff viewport at the last render, for paging.
    view_height: u16,
    responder: oneshot::Sender<Permission>,
}

impl PendingPermission {
    fn scroll_up(&mut self, by: u16) {
        self.scroll = self.scroll.saturating_sub(by);
    }

    fn scroll_down(&mut self, by: u16) {
        self.scroll = self.scroll.saturating_add(by);
    }

    fn page(&self) -> u16 {
        self.view_height.saturating_sub(1).max(1)
    }
}

pub struct AppState {
    items: Vec<TranscriptItem>,
    streaming: String,
    streaming_thinking: String,
    input: String,
    is_running: bool,
    model_name: String,
    current_cancel: Option<CancellationToken>,
    // Set while a tool is awaiting the user's allow/deny answer.
    pending_permission: Option<PendingPermission>,
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
            pending_permission: None,
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

    fn handle_session_event(&mut self, ev: EventKind) {
        match ev {
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
            EventKind::ToolCallStarted(start) => {
                self.flush_streaming();
                self.items.push(TranscriptItem::ToolCall {
                    id: start.tool_call.id.clone(),
                    name: start.tool_call.name.clone(),
                    args: summarize_json(&start.tool_call.args),
                    outcome: None,
                });
            }
            EventKind::ToolCallUpdated(update) => {
                debug!(?update, "EventKind::ToolCallUpdated");
            }
            EventKind::ToolCallFinished(finish) => {
                let rendered = render_outcome(finish.outcome);
                let matched = self.items.iter_mut().rev().find_map(|item| match item {
                    TranscriptItem::ToolCall {
                        id: tool_call_id,
                        name: n,
                        outcome: o @ None,
                        ..
                    } if *tool_call_id == finish.tool_call.id => Some(o),
                    _ => None,
                });
                if let Some(slot) = matched {
                    *slot = Some(rendered);
                } else {
                    self.items.push(TranscriptItem::ToolCall {
                        id: finish.tool_call.id.clone(),
                        name: finish.tool_call.name.clone(),
                        args: String::new(),
                        outcome: Some(rendered),
                    });
                }
            }
        }
    }

    fn respond_permission(&mut self, decision: Permission) {
        if let Some(pending) = self.pending_permission.take() {
            let _ = pending.responder.send(decision);
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
    _id: &'a str,
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

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    }
}

fn centered_rect_pct(pct_x: u16, pct_y: u16, area: Rect) -> Rect {
    centered_rect(area.width * pct_x / 100, area.height * pct_y / 100, area)
}

/// Builds a unified-style diff between `old` and `new`: only the changed regions
/// with a few lines of context, deletions in red and insertions in green, with a
/// marker between hunks. Within a changed line the segments that actually differ
/// are reversed, so a small change in a long line stands out. Returns a single
/// line when there is nothing to show.
fn build_diff_lines(old: &str, new: &str) -> Vec<Line<'static>> {
    let dim = Style::default().fg(Color::DarkGray);
    let diff = TextDiff::from_lines(old, new);
    let groups = diff.grouped_ops(3);
    if groups.is_empty() {
        return vec![Line::from(Span::styled("(no changes)", dim))];
    }
    let mut lines = Vec::new();
    for (i, group) in groups.iter().enumerate() {
        if i > 0 {
            lines.push(Line::from(Span::styled("  ⋯", dim)));
        }
        for op in group {
            for change in diff.iter_inline_changes(op) {
                let (sign, base) = match change.tag() {
                    ChangeTag::Delete => ("-", Style::default().fg(Color::Red)),
                    ChangeTag::Insert => ("+", Style::default().fg(Color::Green)),
                    ChangeTag::Equal => (" ", dim),
                };
                let mut spans = vec![Span::styled(format!("{sign} "), base)];
                for (emphasized, value) in change.iter_strings_lossy() {
                    let value = value.strip_suffix('\n').unwrap_or(&value);
                    let value = value.strip_suffix('\r').unwrap_or(value);
                    let style = if emphasized {
                        base.add_modifier(Modifier::REVERSED)
                    } else {
                        base
                    };
                    spans.push(Span::styled(value.to_owned(), style));
                }
                lines.push(Line::from(spans));
            }
        }
    }
    lines
}

fn prompt_border() -> Style {
    Style::default()
        .fg(Color::Yellow)
        .add_modifier(Modifier::BOLD)
}

fn render_permission_prompt(f: &mut Frame, area: Rect, pending: &mut PendingPermission) {
    if pending.diff.is_some() {
        render_edit_prompt(f, area, pending);
    } else {
        render_simple_prompt(f, area, pending);
    }
}

/// The compact allow/deny popup for tools without a diff to preview.
fn render_simple_prompt(f: &mut Frame, area: Rect, pending: &PendingPermission) {
    let popup = centered_rect(60, 9, area);
    f.render_widget(Clear, popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(prompt_border());
    let mut lines = vec![
        Line::from("The agent wants to run this tool:"),
        Line::from(""),
        Line::from(Span::styled(pending.tool_name.clone(), prompt_border())),
    ];
    if !pending.args.is_empty() {
        lines.push(Line::from(Span::styled(
            pending.args.clone(),
            Style::default().fg(Color::DarkGray),
        )));
    }
    lines.push(Line::from(""));
    lines.push(allow_deny_line());
    let prompt = Paragraph::new(Text::from(lines))
        .block(block.title(Span::styled(" Permission required ", prompt_border())))
        .wrap(Wrap { trim: false });
    f.render_widget(prompt, popup);
}

/// The roomy popup that previews an edit as a scrollable before/after diff.
fn render_edit_prompt(f: &mut Frame, area: Rect, pending: &mut PendingPermission) {
    let popup = centered_rect_pct(80, 80, area);
    f.render_widget(Clear, popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(prompt_border())
        .title(Span::styled(" Approve edit ", prompt_border()));
    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(inner);

    let path = pending.path.as_deref().unwrap_or_default();
    let header = Paragraph::new(Text::from(vec![
        Line::from(vec![
            Span::raw("The agent wants to edit "),
            Span::styled(path.to_owned(), prompt_border()),
        ]),
        Line::from(""),
    ]));
    f.render_widget(header, chunks[0]);

    let body_area = chunks[1];
    pending.view_height = body_area.height;
    let lines = pending.diff.clone().unwrap_or_default();
    let body = Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false });
    let total = body.line_count(body_area.width);
    let max_scroll = total
        .saturating_sub(body_area.height as usize)
        .min(u16::MAX as usize) as u16;
    if pending.scroll > max_scroll {
        pending.scroll = max_scroll;
    }
    f.render_widget(body.scroll((pending.scroll, 0)), body_area);

    let mut footer = allow_deny_line();
    if max_scroll > 0 {
        footer.spans.push(Span::styled(
            "    PgUp/PgDn scroll",
            Style::default().fg(Color::DarkGray),
        ));
    }
    f.render_widget(Paragraph::new(footer), chunks[2]);
}

fn allow_deny_line() -> Line<'static> {
    Line::from(vec![
        Span::styled(
            "[y]",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" allow once    "),
        Span::styled(
            "[n/esc]",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" deny"),
    ])
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
                id,
                name,
                args,
                outcome,
            } => {
                lines.extend(render_tool_call(id, name, args, outcome.as_ref()));
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

    // Modal permission prompt, drawn last so it sits above the transcript.
    if let Some(pending) = state.pending_permission.as_mut() {
        render_permission_prompt(f, area, pending);
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
                        if state.pending_permission.is_some() {
                            match (code, modifiers) {
                                (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                                    state.respond_permission(Permission::Deny);
                                    break;
                                }
                                (KeyCode::Char('y') | KeyCode::Char('Y'), _) => {
                                    state.respond_permission(Permission::AllowOnce);
                                }
                                (KeyCode::Char('n') | KeyCode::Char('N'), _)
                                | (KeyCode::Esc, _) => {
                                    state.respond_permission(Permission::Deny);
                                }
                                (KeyCode::Up, _) => {
                                    if let Some(p) = state.pending_permission.as_mut() {
                                        p.scroll_up(1);
                                    }
                                }
                                (KeyCode::Down, _) => {
                                    if let Some(p) = state.pending_permission.as_mut() {
                                        p.scroll_down(1);
                                    }
                                }
                                (KeyCode::PageUp, _) => {
                                    if let Some(p) = state.pending_permission.as_mut() {
                                        p.scroll_up(p.page());
                                    }
                                }
                                (KeyCode::PageDown, _) => {
                                    if let Some(p) = state.pending_permission.as_mut() {
                                        p.scroll_down(p.page());
                                    }
                                }
                                _ => {}
                            }
                        } else {
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
                    }
                    Event::Mouse(MouseEvent { kind, .. }) => {
                        match kind {
                            MouseEventKind::ScrollUp => {
                                match state.pending_permission.as_mut() {
                                    Some(p) => p.scroll_up(3),
                                    None => state.scroll_up(3),
                                }
                            }
                            MouseEventKind::ScrollDown => {
                                match state.pending_permission.as_mut() {
                                    Some(p) => p.scroll_down(3),
                                    None => state.scroll_down(3),
                                }
                            }
                            _ => {}
                        }
                    }
                    Event::Resize(_, _) => {}
                    _ => {}
                }
            }

            maybe_cmd = ui_rx.recv() => {
                match maybe_cmd {
                    Some(UiCommand::Event(ev)) => state.handle_session_event(ev),
                    Some(UiCommand::PermissionRequest { request, responder }) => {
                        let edit = EditTextFileProposal::from_proposal(&request.proposal);
                        let (path, diff) = match edit {
                            Some(edit) => (
                                Some(edit.path.display().to_string()),
                                Some(build_diff_lines(&edit.old_content, &edit.new_content)),
                            ),
                            None => (None, None),
                        };
                        state.pending_permission = Some(PendingPermission {
                            tool_name: request.tool_call.name.clone(),
                            args: summarize_json(&request.tool_call.args),
                            path,
                            diff,
                            scroll: 0,
                            view_height: 0,
                            responder,
                        });
                    }
                    None => {}
                }
            }
        }
    }

    Ok(())
}
