use futures_util::StreamExt;
use serde_json::Value;
use std::env;
use std::fs::File;
use std::io::{self, Write};
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::io::AsyncReadExt;
use tokio::net::TcpStream;
use tokio_tungstenite::connect_async;

const WS_URL: &str = "ws://127.0.0.1:49123";
const TCP_ADDR: &str = "127.0.0.1:49123";

#[derive(Debug)]
struct Args {
    output: PathBuf,
    seconds: u64,
    raw_chunks: bool,
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let args = parse_args();
    let mut file = File::create(&args.output)?;

    writeln!(file, "Rocket League Stats API debug capture")?;
    writeln!(file, "started_unix_ms={}", now_ms())?;
    writeln!(file, "target_ws={WS_URL}")?;
    writeln!(file, "target_tcp={TCP_ADDR}")?;
    writeln!(file, "capture_seconds={}", args.seconds)?;
    writeln!(file, "raw_chunks={}", args.raw_chunks)?;
    writeln!(file)?;

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
            writeln!(file, "connected_transport=websocket")?;
            capture_websocket(ws_stream, &mut file, deadline).await?;
        }
        Err(error) if error.to_string().contains("invalid HTTP version") => {
            println!("Stats API appears to be raw TCP. Connecting via TCP.");
            writeln!(file, "websocket_probe_error={error}")?;
            writeln!(file, "connected_transport=tcp")?;
            capture_tcp(&mut file, deadline, args.raw_chunks).await?;
        }
        Err(error) => {
            writeln!(file, "connection_error={error}")?;
            eprintln!("Could not connect to Rocket League Stats API: {error}");
            eprintln!("Make sure Rocket League is running and PacketSendRate is greater than 0.");
        }
    }

    writeln!(file)?;
    writeln!(file, "finished_unix_ms={}", now_ms())?;
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
                    write_payload(file, "websocket", text)?;
                }
            }
            Ok(Some(Err(error))) => {
                writeln!(file, "websocket_error={error}")?;
                return Ok(());
            }
            Ok(None) => {
                writeln!(file, "websocket_closed=true")?;
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
    let mut leftover = String::new();

    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return Ok(());
        }

        let n = match tokio::time::timeout(remaining, stream.read(&mut buffer)).await {
            Ok(Ok(0)) => {
                writeln!(file, "tcp_closed=true")?;
                return Ok(());
            }
            Ok(Ok(n)) => n,
            Ok(Err(error)) => {
                writeln!(file, "tcp_error={error}")?;
                return Ok(());
            }
            Err(_) => return Ok(()),
        };

        let chunk = String::from_utf8_lossy(&buffer[..n]);
        if raw_chunks {
            writeln!(
                file,
                "\n--- raw tcp chunk unix_ms={} bytes={} ---",
                now_ms(),
                n
            )?;
            writeln!(file, "{chunk}")?;
        }

        let text = format!("{leftover}{chunk}");
        leftover.clear();

        let mut start = 0;
        let mut depth = 0;
        let mut in_string = false;
        let mut escaped = false;

        for (i, c) in text.char_indices() {
            if escaped {
                escaped = false;
                continue;
            }
            match c {
                '\\' if in_string => escaped = true,
                '"' => in_string = !in_string,
                '{' if !in_string => {
                    if depth == 0 {
                        start = i;
                    }
                    depth += 1;
                }
                '}' if !in_string && depth > 0 => {
                    depth -= 1;
                    if depth == 0 {
                        write_payload(file, "tcp-json-object", &text[start..=i])?;
                    }
                }
                _ => {}
            }
        }

        if depth > 0 {
            leftover = text[start..].to_string();
        }
    }
}

fn write_payload(file: &mut File, transport: &str, text: &str) -> io::Result<()> {
    writeln!(
        file,
        "\n=== payload unix_ms={} transport={} bytes={} ===",
        now_ms(),
        transport,
        text.len()
    )?;
    writeln!(file, "{text}")?;

    match serde_json::from_str::<Value>(text) {
        Ok(json) => {
            if let Ok(pretty) = serde_json::to_string_pretty(&json) {
                writeln!(file, "\n--- pretty json ---")?;
                writeln!(file, "{pretty}")?;
            }
            write_summary(file, &json)?;
        }
        Err(error) => {
            writeln!(file, "\njson_parse_error={error}")?;
        }
    }

    Ok(())
}

fn write_summary(file: &mut File, json: &Value) -> io::Result<()> {
    let event = json["Event"].as_str().unwrap_or("Unknown");
    writeln!(file, "\n--- derived summary ---")?;
    writeln!(file, "event={event}")?;

    if event != "UpdateState" {
        return Ok(());
    }

    let data = json.get("Data").unwrap_or(json);
    let real_data = if let Some(encoded) = data.as_str() {
        writeln!(file, "data_is_json_string=true")?;
        serde_json::from_str::<Value>(encoded).unwrap_or_else(|error| {
            let _ = writeln!(file, "inner_json_parse_error={error}");
            data.clone()
        })
    } else {
        data.clone()
    };

    if let Some(game) = real_data.get("game").or_else(|| real_data.get("Game")) {
        write_optional_str(file, "game.client", game, &["client", "Client"])?;
        write_optional_str(file, "game.me", game, &["me", "Me"])?;
    }

    let Some(players) = real_data
        .get("Players")
        .or_else(|| real_data.get("players"))
        .or(Some(&real_data))
        .and_then(Value::as_array)
    else {
        writeln!(file, "players_array_found=false")?;
        return Ok(());
    };

    writeln!(file, "players_array_found=true")?;
    writeln!(file, "players_len={}", players.len())?;

    for (index, player) in players.iter().enumerate() {
        writeln!(
            file,
            "player[{index}].name={}",
            string_field(player, &["Name", "name"]).unwrap_or("")
        )?;
        writeln!(
            file,
            "player[{index}].primary_id={}",
            string_field(player, &["PrimaryId", "primaryId", "primary_id"]).unwrap_or("")
        )?;
        writeln!(
            file,
            "player[{index}].team={:?}",
            number_field(player, &["TeamNum", "teamNum", "Team", "team"])
        )?;
        writeln!(
            file,
            "player[{index}].boost={:?}",
            number_field(player, &["Boost", "boost"])
        )?;
        writeln!(
            file,
            "player[{index}].is_local_flags=IsLocalPlayer:{:?}, isLocalPlayer:{:?}, isMe:{:?}",
            player["IsLocalPlayer"].as_bool(),
            player["isLocalPlayer"].as_bool(),
            player["isMe"].as_bool()
        )?;
    }

    Ok(())
}

fn write_optional_str(
    file: &mut File,
    label: &str,
    value: &Value,
    keys: &[&str],
) -> io::Result<()> {
    if let Some(field) = string_field(value, keys) {
        writeln!(file, "{label}={field}")?;
    }
    Ok(())
}

fn string_field<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter().find_map(|key| value[*key].as_str())
}

fn number_field(value: &Value, keys: &[&str]) -> Option<u64> {
    keys.iter().find_map(|key| {
        value[*key]
            .as_u64()
            .or_else(|| value[*key].as_str()?.parse().ok())
    })
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}
