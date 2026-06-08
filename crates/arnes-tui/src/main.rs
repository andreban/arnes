// Copyright 2026 Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0

use std::{io, sync::{Arc, Mutex}};

use agent_rig::models::gemini::GeminiModel;
use arnes_core::{ModelKey, Session};
use clap::Parser;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use geologia::prelude::ThinkingConfig;
use ratatui::{Terminal, backend::CrosstermBackend};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

mod app;
mod frontend;
mod host;

use frontend::{TuiFrontend, UiCommand};

use crate::host::tui_host;

#[derive(Parser)]
#[command(about = "arnes — a local AI coding agent")]
struct Args {
    /// Gemini model identifier
    #[arg(long, default_value = "gemini-3.5-flash")]
    model: String,

    /// Gemini API key
    #[arg(long, env = "GEMINI_API_KEY")]
    api_key: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();
    let args = Args::parse();

    let log_file = std::fs::File::create("arnes-tui.log")?;
    let filter = tracing_subscriber::EnvFilter::new(
        "off,ollama_rs=debug,geologia=debug,agent_rig=debug,arnes_core=debug,arnes=debug",
    );
    tracing_subscriber::fmt()
        .with_writer(Mutex::new(log_file))
        .with_env_filter(filter)
        .with_ansi(false)
        .init();

    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), DisableMouseCapture, LeaveAlternateScreen);
        original_hook(info);
    }));

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;

    let result = run(&args, &mut terminal).await;

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        DisableMouseCapture,
        LeaveAlternateScreen
    )?;
    terminal.show_cursor()?;

    result
}

async fn run(
    args: &Args,
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let thinking_config = ThinkingConfig {
        include_thoughts: true,
        thinking_level: Some(geologia::prelude::ThinkingLevel::High),
        ..Default::default()
    };
    let model = Arc::new(
        GeminiModel::builder(&args.api_key, &args.model)
            .thinking_config(thinking_config)
            .build(),
    );
    let model_key = ModelKey {
        provider: "gemini".into(),
        model_id: args.model.clone(),
    };

    let (ui_tx, ui_rx) = mpsc::unbounded_channel::<UiCommand>();
    let frontend = Arc::new(TuiFrontend::new(ui_tx));
    let host = tui_host();

    let session = Session::new(frontend, host, model, model_key);

    let (prompt_tx, mut prompt_rx) = mpsc::channel::<(String, CancellationToken)>(1);

    tokio::spawn(async move {
        let mut session = session;
        while let Some((text, cancel)) = prompt_rx.recv().await {
            let _ = session.prompt(text, cancel).await;
        }
    });

    app::run(terminal, ui_rx, prompt_tx, args.model.clone()).await?;

    Ok(())
}
