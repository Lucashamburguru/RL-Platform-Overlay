use futures_util::StreamExt;
use serde_json::Value;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
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
    leftover: Vec<u8>,
}

impl TcpJsonSplitter {
    /// Feeds a new byte chunk from the TCP stream and returns any completed JSON payloads.
    ///
    /// If a JSON object is only partially received, its bytes are retained in an internal
    /// buffer (`leftover`) and will be completed by subsequent calls to `push`.
    pub fn push(&mut self, chunk: &[u8]) -> Vec<String> {
        self.leftover.extend_from_slice(chunk);
        let mut payloads = Vec::new();
        let mut start = 0;
        let mut depth = 0;
        let mut in_string = false;
        let mut escaped = false;

        for i in 0..self.leftover.len() {
            let b = self.leftover[i];
            if escaped {
                escaped = false;
                continue;
            }
            match b {
                b'\\' if in_string => escaped = true,
                b'"' => in_string = !in_string,
                b'{' if !in_string => {
                    if depth == 0 {
                        start = i;
                    }
                    depth += 1;
                }
                b'}' if !in_string && depth > 0 => {
                    depth -= 1;
                    if depth == 0 {
                        let bytes = &self.leftover[start..=i];
                        let s = String::from_utf8_lossy(bytes).into_owned();
                        payloads.push(s);
                    }
                }
                _ => {}
            }
        }

        if depth > 0 {
            self.leftover = self.leftover[start..].to_vec();
        } else {
            self.leftover.clear();
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
        tokio::fs::create_dir_all(parent).await?;
    }

    let mut file = File::create(output).await?;
    file.write_all(b"Rocket League Stats API debug capture\n")
        .await?;
    file.write_all(format!("started_unix_ms={}\n", now_ms()).as_bytes())
        .await?;
    file.write_all(format!("target_ws={WS_URL}\n").as_bytes())
        .await?;
    file.write_all(format!("target_tcp={TCP_ADDR}\n").as_bytes())
        .await?;
    file.write_all(format!("capture_seconds={seconds}\n").as_bytes())
        .await?;
    file.write_all(b"\n").await?;

    let deadline = tokio::time::Instant::now() + Duration::from_secs(seconds);
    match connect_async(WS_URL).await {
        Ok((ws_stream, _)) => {
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
            file.write_all(format!("websocket_probe_error={error}\n").as_bytes())
                .await?;
            file.write_all(b"connected_transport=tcp\n").await?;
            capture_tcp(&mut file, deadline).await?;
        }
        Err(error) => {
            file.write_all(format!("connection_error={error}\n").as_bytes())
                .await?;
        }
    }

    file.write_all(b"\n").await?;
    file.write_all(format!("finished_unix_ms={}\n", now_ms()).as_bytes())
        .await?;
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

        for payload in splitter.push(&buffer[..n]) {
            write_payload(file, "tcp-json-object", &payload).await?;
        }
    }
}

async fn write_payload(file: &mut File, source: &str, text: &str) -> io::Result<()> {
    file.write_all(format!("\n--- payload source={source} unix_ms={} ---\n", now_ms()).as_bytes())
        .await?;
    file.write_all(format!("{text}\n").as_bytes()).await?;

    match serde_json::from_str::<Value>(text) {
        Ok(json) => {
            let event = json["Event"].as_str().unwrap_or("Unknown");
            let data = crate::json_utils::decode_json_string_value(&json["Data"]);
            let player_count = data
                .get("Players")
                .or_else(|| data.get("players"))
                .and_then(Value::as_array)
                .map_or(0, Vec::len);
            file.write_all(format!("summary_event={event}\n").as_bytes())
                .await?;
            file.write_all(format!("summary_player_count={player_count}\n").as_bytes())
                .await?;
        }
        Err(error) => {
            file.write_all(format!("summary_parse_error={error}\n").as_bytes())
                .await?;
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
        assert!(splitter.push(b"{\"Event\":\"Update").is_empty());
        let payloads = splitter.push(b"State\",\"Data\":{\"Players\":[]}}");
        assert_eq!(payloads.len(), 1);
        assert_eq!(
            payloads[0],
            r#"{"Event":"UpdateState","Data":{"Players":[]}}"#
        );
    }

    #[test]
    fn tcp_splitter_handles_braces_inside_strings() {
        let mut splitter = TcpJsonSplitter::default();
        let payloads = splitter.push(b"{\"Event\":\"UpdateState\",\"Data\":{\"Name\":\"A } B\"}}");
        assert_eq!(payloads.len(), 1);
    }
}
