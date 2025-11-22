#[cfg(target_arch = "wasm32")]
fn main() {}

#[cfg(not(target_arch = "wasm32"))]
use clap::Parser;
#[cfg(not(target_arch = "wasm32"))]
use futures::{SinkExt, StreamExt};
#[cfg(not(target_arch = "wasm32"))]
use std::path::PathBuf;
#[cfg(not(target_arch = "wasm32"))]
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
#[cfg(not(target_arch = "wasm32"))]
use tokio::net::TcpListener;
#[cfg(not(target_arch = "wasm32"))]
use tokio::process::Command;
#[cfg(not(target_arch = "wasm32"))]
use tokio_tungstenite::{accept_async, tungstenite::Message};

/// Bridge between the browser and cold-clear-2 via TBP over stdin/stdout.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Parser, Debug, Clone)]
struct Opts {
    /// Address to listen for websocket connections (browser connects here)
    #[arg(long, default_value = "127.0.0.1:9000")]
    listen: String,
    /// Path to cold-clear-2 executable
    #[arg(long, default_value = "cold-clear-2/target/release/cold-clear-2.exe")]
    bot_path: PathBuf,
    /// Optional path to bot config JSON passed to cold-clear-2
    #[arg(long)]
    bot_config: Option<PathBuf>,
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();
    let listener = TcpListener::bind(&opts.listen).await?;
    println!("Bot bridge listening on ws://{}", opts.listen);

    loop {
        let (stream, addr) = listener.accept().await?;
        println!("WS connected: {}", addr);
        let opts = opts.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_conn(stream, opts).await {
                eprintln!("connection error {}: {:?}", addr, e);
            }
        });
    }
}

#[cfg(not(target_arch = "wasm32"))]
async fn handle_conn(stream: tokio::net::TcpStream, opts: Opts) -> anyhow::Result<()> {
    let ws_stream = accept_async(stream).await?;
    let (mut ws_tx, mut ws_rx) = ws_stream.split();

    // Spawn cold-clear-2
    let mut cmd = Command::new(&opts.bot_path);
    if let Some(cfg) = opts.bot_config.as_ref() {
        cmd.arg("--config").arg(cfg);
    }
    let mut child = cmd.stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).spawn()?;
    let mut bot_stdin = child
        .stdin
        .take()
        .ok_or_else(|| anyhow::anyhow!("failed to open bot stdin"))?;
    let bot_stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("failed to open bot stdout"))?;

    let mut bot_reader = BufReader::new(bot_stdout).lines();
    let (bot_tx, mut bot_rx) = tokio::sync::mpsc::unbounded_channel::<String>();

    tokio::spawn(async move {
        while let Ok(Some(line)) = bot_reader.next_line().await {
            if bot_tx.send(line).is_err() {
                break;
            }
        }
    });

    // Forward ws <-> bot
    loop {
        tokio::select! {
            Some(line) = bot_rx.recv() => {
                ws_tx.send(Message::Text(line)).await?;
            }
            Some(msg) = ws_rx.next() => {
                match msg {
                    Ok(Message::Text(t)) => {
                        bot_stdin.write_all(t.as_bytes()).await?;
                        bot_stdin.write_all(b"\n").await?;
                    }
                    Ok(Message::Close(_)) => break,
                    Ok(Message::Binary(_)) => {}
                    _ => {}
                }
            }
            else => break,
        }
    }

    let _ = bot_stdin.shutdown().await;
    let _ = child.kill().await;
    Ok(())
}
