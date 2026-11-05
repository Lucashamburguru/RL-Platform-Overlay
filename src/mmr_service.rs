use crate::mmr::MmrError;
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::Path;
use std::sync::LazyLock;
use std::time::Duration;

pub(crate) const SERVICE_URL: &str = "https://files.pengo.me/api/rl-mmr/v1";
static CLIENT: LazyLock<Result<wreq::Client, String>> = LazyLock::new(|| {
    wreq::Client::builder()
        .timeout(Duration::from_secs(15))
        .redirect(wreq::redirect::Policy::none())
        .user_agent(concat!("RL-Platform-Overlay/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|error| error.to_string())
});
static CREDENTIAL_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[derive(Serialize, Deserialize)]
struct Installation {
    installation_key: String,
}

fn installation_key(path: &Path) -> Result<String, MmrError> {
    let _guard = CREDENTIAL_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let read = || -> Result<String, MmrError> {
        let bytes = std::fs::read(path).map_err(storage_error)?;
        let installation: Installation = serde_json::from_slice(&bytes).map_err(|_| {
            MmrError::ProviderResponse("Could not read the saved rank service credential.".into())
        })?;
        let key = installation.installation_key;
        if key.len() != 48
            || !key.starts_with("rlpo_")
            || !key[5..]
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            return Err(MmrError::ProviderResponse(
                "The saved rank service credential is invalid.".into(),
            ));
        }
        Ok(key)
    };
    if path.exists() {
        // Never replace a rejected or revoked credential with a new identity.
        return read();
    }
    let mut random = [0; 32];
    getrandom::fill(&mut random).map_err(|_| {
        MmrError::ProviderResponse("Could not create a rank service credential.".into())
    })?;
    let key = format!(
        "rlpo_{}",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(random)
    );
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(storage_error)?;
    }
    let mut options = std::fs::OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    match options.open(path) {
        Ok(mut file) => {
            let bytes = serde_json::to_vec(&Installation {
                installation_key: key.clone(),
            })?;
            file.write_all(&bytes).map_err(storage_error)?;
            file.sync_all().map_err(storage_error)?;
            Ok(key)
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => read(),
        Err(error) => Err(storage_error(error)),
    }
}

fn storage_error(_: std::io::Error) -> MmrError {
    MmrError::ProviderResponse("Could not save or read the rank service credential.".into())
}

async fn fetch_from(
    client: &wreq::Client,
    host: &str,
    credential_path: &Path,
    player_id: &str,
) -> Result<String, MmrError> {
    let key = installation_key(credential_path)?;
    let url = format!(
        "{host}/get-skills?playerId={}",
        urlencoding::encode(player_id)
    );
    let mut response = client.get(&url).bearer_auth(&key).send().await?;
    if response.status().as_u16() == 401 {
        let registration = client
            .post(format!("{host}/installations"))
            .header("Content-Type", "application/json")
            .body(serde_json::json!({ "installationKey": key }).to_string())
            .send()
            .await?;
        if !registration.status().is_success() {
            return Err(MmrError::HttpStatus(registration.status().as_u16()));
        }
        response = client.get(&url).bearer_auth(&key).send().await?;
    }
    if !response.status().is_success() {
        return Err(MmrError::HttpStatus(response.status().as_u16()));
    }
    response
        .text()
        .await
        .map_err(|error| MmrError::Decode(error.to_string()))
}

pub(crate) async fn fetch_skills(player_id: &str) -> Result<String, MmrError> {
    let client = CLIENT.as_ref().map_err(|_| {
        MmrError::ProviderResponse("Could not initialize the rank service connection.".into())
    })?;
    let path = crate::state::config_dir()
        .ok_or_else(|| {
            MmrError::ProviderResponse(
                "Could not locate the rank service credential directory.".into(),
            )
        })?
        .join("rank-service.json");
    fetch_from(client, SERVICE_URL, &path, player_id).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    async fn fixture(statuses: &[u16]) -> (String, tokio::task::JoinHandle<Vec<String>>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let host = format!("http://{}", listener.local_addr().unwrap());
        let statuses = statuses.to_vec();
        let task = tokio::spawn(async move {
            let mut requests = Vec::new();
            for status in statuses {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut bytes = Vec::new();
                loop {
                    let mut buf = [0; 4096];
                    let n = socket.read(&mut buf).await.unwrap();
                    assert!(n > 0);
                    bytes.extend_from_slice(&buf[..n]);
                    if let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
                        let headers = String::from_utf8_lossy(&bytes[..end]).to_ascii_lowercase();
                        let length: usize = headers
                            .lines()
                            .find_map(|line| line.strip_prefix("content-length: "))
                            .map(|value| value.parse().unwrap())
                            .unwrap_or(0);
                        if bytes.len() >= end + 4 + length {
                            break;
                        }
                    }
                }
                requests.push(String::from_utf8(bytes).unwrap());
                let body = if status == 201 {
                    r#"{"registered":true}"#
                } else {
                    r#"{"playlists":[]}"#
                };
                socket.write_all(format!("HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
            }
            requests
        });
        (host, task)
    }

    #[tokio::test]
    async fn automatically_registers_and_reuses_private_installation_credential() {
        let (host, server) = fixture(&[401, 201, 200, 200]).await;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rank-service.json");
        let client = wreq::Client::builder().no_proxy().build().unwrap();
        fetch_from(&client, &host, &path, "Steam|123|0")
            .await
            .unwrap();
        fetch_from(&client, &host, &path, "Steam|123|0")
            .await
            .unwrap();
        let requests = server.await.unwrap();
        let key = installation_key(&path).unwrap();
        assert!(requests[1].starts_with("POST /installations "));
        let body = requests[1].split("\r\n\r\n").nth(1).unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(body).unwrap()["installationKey"],
            key
        );
        for i in [0, 2, 3] {
            assert!(requests[i].to_ascii_lowercase().contains(&format!(
                "authorization: bearer {}",
                key.to_ascii_lowercase()
            )));
            assert!(!requests[i].lines().next().unwrap().contains(&key));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }

    #[tokio::test]
    async fn revoked_installations_keep_their_identity_and_do_not_register_again() {
        let (host, server) = fixture(&[403]).await;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rank-service.json");
        let key = installation_key(&path).unwrap();
        let client = wreq::Client::builder().no_proxy().build().unwrap();
        assert!(matches!(
            fetch_from(&client, &host, &path, "Steam|123|0").await,
            Err(MmrError::HttpStatus(403))
        ));
        assert_eq!(installation_key(&path).unwrap(), key);
        assert_eq!(server.await.unwrap().len(), 1);
    }
}
