use crate::state::{AppState, VersionCheck};
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;

const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
const LATEST_RELEASE_URL: &str =
    "https://api.github.com/repos/Lucashamburguru/RL-Platform-Overlay/releases/latest";

pub fn start_version_check(state: Arc<AppState>) {
    tokio::spawn(async move {
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        let version_check = match check_latest_release().await {
            Ok(check) => check,
            Err(error) => VersionCheck {
                checked: true,
                error: format!("Version check failed: {error}"),
                ..Default::default()
            },
        };
        state.version_check.store(Arc::new(version_check));
    });
}

async fn check_latest_release() -> Result<VersionCheck, wreq::Error> {
    let client = wreq::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()?;
    let response = client
        .get(LATEST_RELEASE_URL)
        .header("User-Agent", "RL-Platform-Overlay")
        .send()
        .await?;
    let status = response.status();
    if !status.is_success() {
        return Ok(VersionCheck {
            checked: true,
            error: format!("GitHub version check failed with status {status}."),
            ..Default::default()
        });
    }

    let body = response.text().await?;
    let Ok(release) = serde_json::from_str::<Value>(&body) else {
        return Ok(VersionCheck {
            checked: true,
            error: "Could not parse GitHub release response.".to_string(),
            ..Default::default()
        });
    };

    if release["draft"].as_bool().unwrap_or(false)
        || release["prerelease"].as_bool().unwrap_or(false)
    {
        return Ok(VersionCheck {
            checked: true,
            error: "Latest GitHub release is not a public stable release.".to_string(),
            ..Default::default()
        });
    }

    let Some(latest_tag) = latest_release_tag(&release) else {
        return Ok(VersionCheck {
            checked: true,
            error: "No release tag found on GitHub.".to_string(),
            ..Default::default()
        });
    };

    let update_available = compare_versions(&latest_tag, CURRENT_VERSION)
        .is_some_and(|ordering| ordering == std::cmp::Ordering::Greater);

    Ok(VersionCheck {
        checked: true,
        update_available,
        latest_tag: latest_tag.clone(),
        release_url: release["html_url"]
            .as_str()
            .unwrap_or("https://github.com/Lucashamburguru/RL-Platform-Overlay/releases/latest")
            .to_string(),
        error: String::new(),
    })
}

fn latest_release_tag(release: &Value) -> Option<String> {
    let tag = release["tag_name"].as_str()?;
    parse_version(tag)?;
    Some(tag.to_string())
}

fn compare_versions(left: &str, right: &str) -> Option<std::cmp::Ordering> {
    Some(parse_version(left)?.cmp(&parse_version(right)?))
}

fn parse_version(version: &str) -> Option<Vec<u64>> {
    let clean = version
        .trim()
        .trim_start_matches('v')
        .split(['-', '+'])
        .next()?;
    let mut parts = Vec::new();
    for part in clean.split('.') {
        parts.push(part.parse().ok()?);
    }
    Some(parts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_latest_release_tag_uses_release_tag_name() {
        let release = json!({
            "tag_name": "v0.2.0",
            "html_url": "https://github.com/example/repo/releases/tag/v0.2.0"
        });

        assert_eq!(latest_release_tag(&release), Some("v0.2.0".to_string()));
    }

    #[test]
    fn test_latest_release_tag_ignores_non_semver_tag_name() {
        let release = json!({ "tag_name": "latest" });

        assert_eq!(latest_release_tag(&release), None);
    }

    #[test]
    fn test_compare_versions_strips_v_prefix() {
        assert_eq!(
            compare_versions("v0.2.0", "0.1.4"),
            Some(std::cmp::Ordering::Greater)
        );
        assert_eq!(
            compare_versions("v0.1.4", "0.1.4"),
            Some(std::cmp::Ordering::Equal)
        );
    }
}
