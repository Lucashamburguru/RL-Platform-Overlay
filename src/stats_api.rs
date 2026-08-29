use futures_util::StreamExt;
use serde_json::Value;
use std::collections::VecDeque;
use std::io;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_tungstenite::connect_async;

pub const WS_URL: &str = "ws://127.0.0.1:49123";
pub const TCP_ADDR: &str = "127.0.0.1:49123";
const RECENT_LOG_WINDOW_MS: u128 = 120_000;
const UPDATE_STATE_SAMPLE_INTERVAL_MS: u128 = 1_000;
const RECENT_LOG_MAX_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, Debug)]
struct RecentStatsApiEntry {
    unix_ms: u128,
    source: &'static str,
    event: Arc<str>,
    payload: Arc<str>,
}

#[derive(Clone, Debug, Default)]
pub struct RecentStatsApiSnapshot {
    entries: Vec<RecentStatsApiEntry>,
    detected_mode: String,
    detected_mode_source: String,
}

impl RecentStatsApiSnapshot {
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn with_detected_mode(mut self, mode: &str, source: &str) -> Self {
        self.detected_mode = mode.to_string();
        self.detected_mode_source = source.to_string();
        self
    }

    fn render(&self, generated_unix_ms: u128) -> String {
        let mut output = String::new();
        output.push_str("Rocket League Stats API recent issue log\n");
        output.push_str(&format!("generated_unix_ms={generated_unix_ms}\n"));
        output.push_str(&format!("app_version={}\n", env!("CARGO_PKG_VERSION")));
        output.push_str("privacy=identifiable\n");
        output.push_str(
            "warning=May contain player names, account identifiers, and match identifiers.\n",
        );
        output.push_str("capture_window_seconds=120\n");
        output.push_str("update_state_sampling_seconds=1\n");
        output.push_str(&format!("detected_mode={}\n", self.detected_mode));
        output.push_str(&format!(
            "detected_mode_source={}\n",
            self.detected_mode_source
        ));
        output.push_str(&format!("payload_count={}\n", self.entries.len()));

        for entry in &self.entries {
            output.push_str(&format!(
                "\n--- payload source={} event={} unix_ms={} ---\n",
                entry.source, entry.event, entry.unix_ms
            ));
            output.push_str(&entry.payload);
            output.push('\n');
        }
        output
    }
}

#[derive(Debug, Default)]
pub struct RecentStatsApiLog {
    entries: VecDeque<RecentStatsApiEntry>,
    total_bytes: usize,
    last_update_state_unix_ms: u128,
}

impl RecentStatsApiLog {
    pub fn record(&mut self, source: StatsApiTransport, event: &str, payload: &str, unix_ms: u128) {
        self.remove_expired(unix_ms);
        if event == "UpdateState"
            && self.last_update_state_unix_ms != 0
            && unix_ms.saturating_sub(self.last_update_state_unix_ms)
                < UPDATE_STATE_SAMPLE_INTERVAL_MS
        {
            return;
        }
        if event == "UpdateState" {
            self.last_update_state_unix_ms = unix_ms;
        }

        let payload_bytes = payload.len();
        if payload_bytes > RECENT_LOG_MAX_BYTES {
            return;
        }
        self.entries.push_back(RecentStatsApiEntry {
            unix_ms,
            source: match source {
                StatsApiTransport::WebSocket => "websocket",
                StatsApiTransport::Tcp => "tcp-json-object",
                StatsApiTransport::Unknown => "unknown",
            },
            event: Arc::from(event),
            payload: Arc::from(payload),
        });
        self.total_bytes = self.total_bytes.saturating_add(payload_bytes);
        while self.total_bytes > RECENT_LOG_MAX_BYTES {
            self.pop_front();
        }
    }

    pub fn snapshot(&mut self, unix_ms: u128) -> RecentStatsApiSnapshot {
        self.remove_expired(unix_ms);
        RecentStatsApiSnapshot {
            entries: self.entries.iter().cloned().collect(),
            ..Default::default()
        }
    }

    fn remove_expired(&mut self, unix_ms: u128) {
        while self
            .entries
            .front()
            .is_some_and(|entry| unix_ms.saturating_sub(entry.unix_ms) > RECENT_LOG_WINDOW_MS)
        {
            self.pop_front();
        }
    }

    fn pop_front(&mut self) {
        if let Some(entry) = self.entries.pop_front() {
            self.total_bytes = self.total_bytes.saturating_sub(entry.payload.len());
        }
    }
}

pub fn default_recent_log_path(config_dir: &Path) -> PathBuf {
    config_dir
        .join("captures")
        .join(format!("rl_stats_issue_log_{}.txt", now_ms()))
}

pub fn save_recent_stats_api_snapshot(
    snapshot: &RecentStatsApiSnapshot,
    output: &Path,
) -> io::Result<()> {
    if snapshot.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "No recent Stats API events are available yet",
        ));
    }
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(output)?;
    file.write_all(snapshot.render(now_ms()).as_bytes())
}

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
    depth: usize,
    in_string: bool,
    escaped: bool,
    start_idx: usize,
    scan_idx: usize,
}

impl TcpJsonSplitter {
    /// Feeds a new byte chunk from the TCP stream and returns any completed JSON payloads.
    ///
    /// If a JSON object is only partially received, its bytes are retained in an internal
    /// buffer (`leftover`) and will be completed by subsequent calls to `push`.
    pub fn push(&mut self, chunk: &[u8]) -> Vec<String> {
        const MAX_BUFFER_SIZE: usize = 5 * 1024 * 1024;
        if self.leftover.len() + chunk.len() > MAX_BUFFER_SIZE {
            log::warn!("TcpJsonSplitter buffer exceeded limit, clearing buffer.");
            self.leftover.clear();
            self.depth = 0;
            self.in_string = false;
            self.escaped = false;
            self.start_idx = 0;
            self.scan_idx = 0;
        }

        self.leftover.extend_from_slice(chunk);
        let mut payloads = Vec::new();

        while self.scan_idx < self.leftover.len() {
            let b = self.leftover[self.scan_idx];
            if self.escaped {
                self.escaped = false;
                self.scan_idx += 1;
                continue;
            }
            match b {
                b'\\' if self.in_string => self.escaped = true,
                b'"' => self.in_string = !self.in_string,
                b'{' if !self.in_string => {
                    if self.depth == 0 {
                        self.start_idx = self.scan_idx;
                    }
                    self.depth += 1;
                }
                b'}' if !self.in_string && self.depth > 0 => {
                    self.depth -= 1;
                    if self.depth == 0 {
                        let bytes = &self.leftover[self.start_idx..=self.scan_idx];
                        let s = String::from_utf8_lossy(bytes).into_owned();
                        payloads.push(s);
                    }
                }
                _ => {}
            }
            self.scan_idx += 1;
        }

        if self.depth > 0 {
            if self.start_idx > 0 {
                self.leftover = self.leftover[self.start_idx..].to_vec();
                self.scan_idx -= self.start_idx;
                self.start_idx = 0;
            }
        } else {
            self.leftover.clear();
            self.scan_idx = 0;
            self.start_idx = 0;
            self.in_string = false;
            self.escaped = false;
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

    #[test]
    fn recent_log_samples_state_updates_and_keeps_discrete_events() {
        let mut log = RecentStatsApiLog::default();
        log.record(
            StatsApiTransport::WebSocket,
            "UpdateState",
            r#"{"Event":"UpdateState","frame":1}"#,
            100,
        );
        log.record(
            StatsApiTransport::WebSocket,
            "UpdateState",
            r#"{"Event":"UpdateState","frame":2}"#,
            500,
        );
        log.record(
            StatsApiTransport::WebSocket,
            "GoalScored",
            r#"{"Event":"GoalScored"}"#,
            600,
        );
        log.record(
            StatsApiTransport::WebSocket,
            "UpdateState",
            r#"{"Event":"UpdateState","frame":3}"#,
            1_200,
        );

        let snapshot = log.snapshot(1_200);
        let rendered = snapshot.render(1_200);
        assert_eq!(snapshot.len(), 3);
        assert!(rendered.contains("frame\":1"));
        assert!(!rendered.contains("frame\":2"));
        assert!(rendered.contains("GoalScored"));
        assert!(rendered.contains("frame\":3"));
    }

    #[test]
    fn recent_log_discards_entries_outside_the_rolling_window() {
        let mut log = RecentStatsApiLog::default();
        log.record(
            StatsApiTransport::Tcp,
            "RoundStarted",
            r#"{"Event":"RoundStarted"}"#,
            1,
        );

        assert!(log.snapshot(RECENT_LOG_WINDOW_MS + 2).is_empty());
    }

    #[test]
    fn recent_log_export_includes_detection_context_and_does_not_overwrite() {
        let temp_dir = tempfile::tempdir().unwrap();
        let output = temp_dir.path().join("issue-log.txt");
        let mut log = RecentStatsApiLog::default();
        log.record(
            StatsApiTransport::WebSocket,
            "UpdateState",
            r#"{"Event":"UpdateState","Data":{"Playlist":"Ranked Doubles 2v2"}}"#,
            1_000,
        );
        let snapshot = log
            .snapshot(1_000)
            .with_detected_mode("2v2", "playlist_metadata");

        save_recent_stats_api_snapshot(&snapshot, &output).unwrap();
        let contents = std::fs::read_to_string(&output).unwrap();
        assert!(contents.contains("privacy=identifiable"));
        assert!(contents.contains("detected_mode=2v2"));
        assert!(contents.contains("detected_mode_source=playlist_metadata"));
        assert!(contents.contains("Ranked Doubles 2v2"));
        assert_eq!(
            save_recent_stats_api_snapshot(&snapshot, &output)
                .unwrap_err()
                .kind(),
            io::ErrorKind::AlreadyExists
        );

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&output).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }
}
