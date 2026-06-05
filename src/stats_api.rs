use futures_util::StreamExt;
use serde_json::Value;
use std::fs::File;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::io::AsyncReadExt;
use tokio::net::TcpStream;
use tokio_tungstenite::connect_async;

pub const WS_URL: &str = "ws://127.0.0.1:49123";
pub const TCP_ADDR: &str = "127.0.0.1:49123";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StatsApiTransport {
    #[default]
    Unknown,
    WebSocket,
    Tcp,
}

impl StatsApiTransport {
    pub fn label(self) -> &'static str {
        match self {
            Self::Unknown => "Unknown",
            Self::WebSocket => "WebSocket",
            Self::Tcp => "Raw TCP",
        }
    }
}

/// A streaming parser helper that accumulates fragmented TCP buffer streams and splits them
/// into complete, valid JSON object strings.
///
/// Because TCP is stream-oriented and does not guarantee packet boundaries, payloads can arrive
/// split across packets or concatenated together. This splitter uses basic brace-matching
/// depth analysis (while respecting JSON string escapes and double quotes) to identify complete
/// top-level `{ ... }` JSON objects.
#[derive(Clone, Debug, Default)]
pub struct TcpJsonSplitter {
    leftover: String,
}

impl TcpJsonSplitter {
    /// Feeds a new text chunk from the TCP stream and returns any completed JSON payloads.
    ///
    /// If a JSON object is only partially received, its characters are retained in an internal
    /// buffer (`leftover`) and will be completed by subsequent calls to `push`.
    pub fn push(&mut self, chunk: &str) -> Vec<String> {
        let text = format!("{}{}", self.leftover, chunk);
        self.leftover.clear();

        let mut payloads = Vec::new();
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
                        payloads.push(text[start..=i].to_string());
                    }
                }
                _ => {}
            }
        }

        if depth > 0 {
            self.leftover = text[start..].to_string();
        }

        payloads
    }
}

pub fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

pub fn default_capture_path(config_dir: Option<PathBuf>) -> PathBuf {
    let file_name = format!("rl_stats_capture_{}.txt", now_ms());
    config_dir
        .map(|dir| dir.join("captures").join(&file_name))
        .unwrap_or_else(|| PathBuf::from(file_name))
}

pub async fn capture_to_file(output: &Path, seconds: u64) -> io::Result<()> {
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut file = File::create(output)?;
    writeln!(file, "Rocket League Stats API debug capture")?;
    writeln!(file, "started_unix_ms={}", now_ms())?;
    writeln!(file, "target_ws={WS_URL}")?;
    writeln!(file, "target_tcp={TCP_ADDR}")?;
    writeln!(file, "capture_seconds={seconds}")?;
    writeln!(file)?;

    let deadline = tokio::time::Instant::now() + Duration::from_secs(seconds);
    match connect_async(WS_URL).await {
        Ok((ws_stream, _)) => {
            writeln!(file, "connected_transport=websocket")?;
            capture_websocket(ws_stream, &mut file, deadline).await?;
        }
        Err(error) if error.to_string().contains("invalid HTTP version") => {
            writeln!(file, "websocket_probe_error={error}")?;
            writeln!(file, "connected_transport=tcp")?;
            capture_tcp(&mut file, deadline).await?;
        }
        Err(error) => {
            writeln!(file, "connection_error={error}")?;
        }
    }

    writeln!(file)?;
    writeln!(file, "finished_unix_ms={}", now_ms())?;
    Ok(())
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

async fn capture_tcp(file: &mut File, deadline: tokio::time::Instant) -> io::Result<()> {
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
        for payload in splitter.push(&chunk) {
            write_payload(file, "tcp-json-object", &payload)?;
        }
    }
}

fn write_payload(file: &mut File, source: &str, text: &str) -> io::Result<()> {
    writeln!(
        file,
        "\n--- payload source={source} unix_ms={} ---",
        now_ms()
    )?;
    writeln!(file, "{text}")?;

    match serde_json::from_str::<Value>(text) {
        Ok(json) => {
            let event = json["Event"].as_str().unwrap_or("Unknown");
            let player_count = json["Data"]
                .get("Players")
                .or_else(|| json["Data"].get("players"))
                .and_then(Value::as_array)
                .map_or(0, Vec::len);
            writeln!(file, "summary_event={event}")?;
            writeln!(file, "summary_player_count={player_count}")?;
        }
        Err(error) => {
            writeln!(file, "summary_parse_error={error}")?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tcp_splitter_handles_split_payloads() {
        let mut splitter = TcpJsonSplitter::default();
        assert!(splitter.push(r#"{"Event":"Update"#).is_empty());
        let payloads = splitter.push(r#"State","Data":{"Players":[]}}"#);
        assert_eq!(payloads.len(), 1);
        assert_eq!(
            payloads[0],
            r#"{"Event":"UpdateState","Data":{"Players":[]}}"#
        );
    }

    #[test]
    fn tcp_splitter_handles_braces_inside_strings() {
        let mut splitter = TcpJsonSplitter::default();
        let payloads = splitter.push(r#"{"Event":"UpdateState","Data":{"Name":"A } B"}}"#);
        assert_eq!(payloads.len(), 1);
    }
}
