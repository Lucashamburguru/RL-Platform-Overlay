use crate::state::{AppState, VersionCheck};
use serde_json::Value;
use std::sync::Arc;

const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
const TAGS_URL: &str = "https://api.github.com/repos/Lucashamburguru/RL-Platform-Overlay/tags";

pub fn start_version_check(state: Arc<AppState>) {
    tokio::spawn(async move {
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        let version_check = match check_latest_tag().await {
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

async fn check_latest_tag() -> Result<VersionCheck, wreq::Error> {
    let response = wreq::Client::new()
        .get(TAGS_URL)
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
    let Ok(tags) = serde_json::from_str::<Value>(&body) else {
        return Ok(VersionCheck {
            checked: true,
            error: "Could not parse GitHub tags response.".to_string(),
            ..Default::default()
        });
    };

    let Some(latest_tag) = latest_semver_tag(&tags) else {
        return Ok(VersionCheck {
            checked: true,
            error: "No release tags found on GitHub.".to_string(),
            ..Default::default()
        });
    };

    let update_available = compare_versions(&latest_tag, CURRENT_VERSION)
        .is_some_and(|ordering| ordering == std::cmp::Ordering::Greater);

    Ok(VersionCheck {
        checked: true,
        update_available,
        latest_tag: latest_tag.clone(),
        release_url: format!(
            "https://github.com/Lucashamburguru/RL-Platform-Overlay/releases/tag/{latest_tag}"
        ),
        error: String::new(),
    })
}

fn latest_semver_tag(tags: &Value) -> Option<String> {
    tags.as_array()?
        .iter()
        .filter_map(|tag| tag["name"].as_str())
        .filter(|tag| parse_version(tag).is_some())
        .max_by(|a, b| compare_versions(a, b).unwrap_or(std::cmp::Ordering::Equal))
        .map(ToString::to_string)
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
    fn test_latest_semver_tag_uses_highest_version() {
        let tags = json!([
            { "name": "v0.1.4" },
            { "name": "v0.2.0" },
            { "name": "not-a-version" },
            { "name": "0.1.9" }
        ]);

        assert_eq!(latest_semver_tag(&tags), Some("v0.2.0".to_string()));
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
