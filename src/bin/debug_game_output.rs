use futures_util::StreamExt;
use rl_platform_overlay::json_utils::{decode_json_string_value, number_field, string_field};
use rl_platform_overlay::stats_api::{TCP_ADDR, TcpJsonSplitter, WS_URL, now_ms};
use serde_json::Value;
use std::env;
use std::io;
use std::path::PathBuf;
use std::time::Duration;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_tungstenite::connect_async;

#[derive(Debug)]
struct Args {
    output: PathBuf,
    seconds: u64,
    raw_chunks: bool,
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let args = parse_args();
    let mut file = File::create(&args.output).await?;

    file.write_all(b"Rocket League Stats API debug capture\n")
        .await?;
    file.write_all(format!("started_unix_ms={}\n", now_ms()).as_bytes())
        .await?;
    file.write_all(format!("target_ws={WS_URL}\n").as_bytes())
        .await?;
    file.write_all(format!("target_tcp={TCP_ADDR}\n").as_bytes())
        .await?;
    file.write_all(format!("capture_seconds={}\n", args.seconds).as_bytes())
        .await?;
    file.write_all(format!("raw_chunks={}\n", args.raw_chunks).as_bytes())
        .await?;
    file.write_all(b"\n").await?;

    println!(
        "Capturing Rocket League output for {}s into {}",
        args.seconds,
        args.output.display()
    );
    println!("Start Rocket League with Stats API enabled, then enter a match or free play.");

    let deadline = tokio::time::Instant::now() + Duration::from_secs(args.seconds);

    match connect_async(WS_URL).await {
        Ok((ws_stream, _)) => {
            println!("Connected via WebSocket.");
            file.write_all(b"connected_transport=websocket\n").await?;
            capture_websocket(ws_stream, &mut file, deadline).await?;
        }
        Err(error)
            if matches!(
                error,
                tokio_tungstenite::tungstenite::Error::Protocol(
                    tokio_tungstenite::tungstenite::error::ProtocolError::HttparseError(_)
                )
            ) =>
        {
            println!("Stats API appears to be raw TCP. Connecting via TCP.");
            file.write_all(format!("websocket_probe_error={error}\n").as_bytes())
                .await?;
            file.write_all(b"connected_transport=tcp\n").await?;
            capture_tcp(&mut file, deadline, args.raw_chunks).await?;
        }
        Err(error) => {
            file.write_all(format!("connection_error={error}\n").as_bytes())
                .await?;
            eprintln!("Could not connect to Rocket League Stats API: {error}");
            eprintln!("Make sure Rocket League is running and PacketSendRate is greater than 0.");
        }
    }

    file.write_all(b"\n").await?;
    file.write_all(format!("finished_unix_ms={}\n", now_ms()).as_bytes())
        .await?;
    println!("Capture complete: {}", args.output.display());

    Ok(())
}

fn parse_args() -> Args {
    let mut output = None;
    let mut seconds = 30;
    let mut raw_chunks = false;
    let mut args = env::args().skip(1);

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--output" | "-o" => {
                let Some(value) = args.next() else {
                    print_usage_and_exit();
                };
                output = Some(PathBuf::from(value));
            }
            "--seconds" | "-s" => {
                let Some(value) = args.next() else {
                    print_usage_and_exit();
                };
                seconds = value.parse().unwrap_or_else(|_| {
                    eprintln!("Invalid --seconds value: {value}");
                    std::process::exit(2);
                });
            }
            "--raw-chunks" => raw_chunks = true,
            "--help" | "-h" => print_usage_and_exit(),
            unknown => {
                eprintln!("Unknown argument: {unknown}");
                print_usage_and_exit();
            }
        }
    }

    Args {
        output: output.unwrap_or_else(default_output_path),
        seconds,
        raw_chunks,
    }
}

fn default_output_path() -> PathBuf {
    PathBuf::from(format!("rl_game_output_debug_{}.txt", now_ms()))
}

fn print_usage_and_exit() -> ! {
    eprintln!(
        "Usage: cargo run --bin debug_game_output -- [--seconds 30] [--output file.txt] [--raw-chunks]"
    );
    std::process::exit(2);
}

async fn capture_websocket<S>(
    mut ws_stream: tokio_tungstenite::WebSocketStream<S>,
    file: &mut File,
    deadline: tokio::time::Instant,
) -> io::Result<()>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return Ok(());
        }

        match tokio::time::timeout(remaining, ws_stream.next()).await {
            Ok(Some(Ok(msg))) => {
                if let Ok(text) = msg.to_text() {
                    write_payload(file, "websocket", text).await?;
                }
            }
            Ok(Some(Err(error))) => {
                file.write_all(format!("websocket_error={error}\n").as_bytes())
                    .await?;
                return Ok(());
            }
            Ok(None) => {
                file.write_all(b"websocket_closed=true\n").await?;
                return Ok(());
            }
            Err(_) => return Ok(()),
        }
    }
}

async fn capture_tcp(
    file: &mut File,
    deadline: tokio::time::Instant,
    raw_chunks: bool,
) -> io::Result<()> {
    let mut stream = TcpStream::connect(TCP_ADDR).await?;
    let mut buffer = [0u8; 16384];
    let mut splitter = TcpJsonSplitter::default();

    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return Ok(());
        }

        let n = match tokio::time::timeout(remaining, stream.read(&mut buffer)).await {
            Ok(Ok(0)) => {
                file.write_all(b"tcp_closed=true\n").await?;
                return Ok(());
            }
            Ok(Ok(n)) => n,
            Ok(Err(error)) => {
                file.write_all(format!("tcp_error={error}\n").as_bytes())
                    .await?;
                return Ok(());
            }
            Err(_) => return Ok(()),
        };

        let chunk = String::from_utf8_lossy(&buffer[..n]);
        if raw_chunks {
            file.write_all(
                format!("\n--- raw tcp chunk unix_ms={} bytes={} ---\n", now_ms(), n).as_bytes(),
            )
            .await?;
            file.write_all(format!("{chunk}\n").as_bytes()).await?;
        }

        for payload in splitter.push(&buffer[..n]) {
            write_payload(file, "tcp-json-object", &payload).await?;
        }
    }
}

async fn write_payload(file: &mut File, transport: &str, text: &str) -> io::Result<()> {
    file.write_all(
        format!(
            "\n=== payload unix_ms={} transport={} bytes={} ===\n",
            now_ms(),
            transport,
            text.len()
        )
        .as_bytes(),
    )
    .await?;
    file.write_all(format!("{text}\n").as_bytes()).await?;

    match serde_json::from_str::<Value>(text) {
        Ok(json) => {
            if let Ok(pretty) = serde_json::to_string_pretty(&json) {
                file.write_all(b"\n--- pretty json ---\n").await?;
                file.write_all(format!("{pretty}\n").as_bytes()).await?;
            }
            write_summary(file, &json).await?;
        }
        Err(error) => {
            file.write_all(format!("\njson_parse_error={error}\n").as_bytes())
                .await?;
        }
    }

    Ok(())
}

async fn write_summary(file: &mut File, json: &Value) -> io::Result<()> {
    let event = json["Event"].as_str().unwrap_or("Unknown");
    file.write_all(b"\n--- derived summary ---\n").await?;
    file.write_all(format!("event={event}\n").as_bytes())
        .await?;

    if event != "UpdateState" {
        return Ok(());
    }

    let data = json.get("Data").unwrap_or(json);
    if data.as_str().is_some() {
        file.write_all(b"data_is_json_string=true\n").await?;
    }
    let real_data = decode_json_string_value(data);

    if let Some(game) = real_data.get("game").or_else(|| real_data.get("Game")) {
        write_optional_str(file, "game.client", game, &["client", "Client"]).await?;
        write_optional_str(file, "game.me", game, &["me", "Me"]).await?;
    }

    let Some(players) = real_data
        .get("Players")
        .or_else(|| real_data.get("players"))
        .or(Some(&real_data))
        .and_then(Value::as_array)
    else {
        file.write_all(b"players_array_found=false\n").await?;
        return Ok(());
    };

    file.write_all(b"players_array_found=true\n").await?;
    file.write_all(format!("players_len={}\n", players.len()).as_bytes())
        .await?;

    for (index, player) in players.iter().enumerate() {
        file.write_all(
            format!(
                "player[{index}].name={}\n",
                string_field(player, &["Name", "name"]).unwrap_or("")
            )
            .as_bytes(),
        )
        .await?;
        file.write_all(
            format!(
                "player[{index}].primary_id={}\n",
                string_field(player, &["PrimaryId", "primaryId", "primary_id"]).unwrap_or("")
            )
            .as_bytes(),
        )
        .await?;
        file.write_all(
            format!(
                "player[{index}].team={:?}\n",
                number_field(player, &["TeamNum", "teamNum", "Team", "team"])
            )
            .as_bytes(),
        )
        .await?;
        file.write_all(
            format!(
                "player[{index}].boost={:?}\n",
                number_field(player, &["Boost", "boost"])
            )
            .as_bytes(),
        )
        .await?;
        file.write_all(format!("player[{index}].is_local_flags=IsLocalPlayer:{:?}, isLocalPlayer:{:?}, isMe:{:?}\n",
            player["IsLocalPlayer"].as_bool(),
            player["isLocalPlayer"].as_bool(),
            player["isMe"].as_bool()
        ).as_bytes()).await?;
    }

    Ok(())
}

async fn write_optional_str(
    file: &mut File,
    label: &str,
    value: &Value,
    keys: &[&str],
) -> io::Result<()> {
    if let Some(field) = string_field(value, keys) {
        file.write_all(format!("{label}={field}\n").as_bytes())
            .await?;
    }
    Ok(())
}
