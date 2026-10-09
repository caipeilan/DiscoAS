//! Public GitHub Releases lookup. No authentication, automatic download or installation.
use reqwest::{Client, StatusCode};
use semver::Version;
use serde::{Deserialize, Serialize};
use std::time::Duration;

pub const REPOSITORY_URL: &str = "https://github.com/caipeilan/DiscoAS";
const LATEST_RELEASE_API: &str = "https://api.github.com/repos/caipeilan/DiscoAS/releases/latest";
const MAX_RESPONSE_BYTES: usize = 256 * 1024;

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub current_version: String,
    pub latest_version: Option<String>,
    pub status: &'static str,
    pub release_url: String,
    pub release_notes: String,
    pub published_at: Option<String>,
    pub download_url: Option<String>,
    pub full_download_url: Option<String>,
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    body: Option<String>,
    published_at: Option<String>,
    #[serde(default)]
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

fn no_release(current: &str) -> UpdateInfo {
    UpdateInfo {
        current_version: current.into(),
        latest_version: None,
        status: "no_release",
        release_url: format!("{REPOSITORY_URL}/releases"),
        release_notes: String::new(),
        published_at: None,
        download_url: None,
        full_download_url: None,
    }
}

fn parse_release(current: &str, bytes: &[u8]) -> Result<UpdateInfo, String> {
    if bytes.len() > MAX_RESPONSE_BYTES {
        return Err("错误：更新信息过大".into());
    }
    let current = Version::parse(current).map_err(|_| "错误：当前版本无效")?;
    let release: Release = serde_json::from_slice(bytes).map_err(|_| "错误：更新信息无效")?;
    if release.draft || release.prerelease {
        return Ok(no_release(&current.to_string()));
    }
    let latest = Version::parse(
        release
            .tag_name
            .strip_prefix('v')
            .unwrap_or(&release.tag_name),
    )
    .map_err(|_| "错误：更新版本无效")?;
    if !latest.pre.is_empty() {
        return Ok(no_release(&current.to_string()));
    }
    let expected_release_url = format!("{REPOSITORY_URL}/releases/tag/{}", release.tag_name);
    if release.html_url != expected_release_url {
        return Err("错误：更新来源无效".into());
    }
    let asset = |suffix: &str| {
        let name = format!("DiscoAS_{latest}_x64-setup{suffix}.exe");
        let expected = format!(
            "{REPOSITORY_URL}/releases/download/{}/{name}",
            release.tag_name
        );
        release
            .assets
            .iter()
            .find(|asset| asset.name == name && asset.browser_download_url == expected)
            .map(|asset| asset.browser_download_url.clone())
    };
    Ok(UpdateInfo {
        current_version: current.to_string(),
        latest_version: Some(latest.to_string()),
        status: if latest > current {
            "update_available"
        } else {
            "up_to_date"
        },
        release_url: expected_release_url,
        release_notes: release
            .body
            .unwrap_or_default()
            .chars()
            .take(20_000)
            .collect(),
        published_at: release.published_at,
        download_url: asset(""),
        full_download_url: asset("-full"),
    })
}

pub async fn check_for_updates(current: &str) -> Result<UpdateInfo, String> {
    let client = Client::builder()
        .user_agent(format!("DiscoAS/{current}"))
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(12))
        .build()
        .map_err(|_| "错误：无法检查更新")?;
    let mut response = client
        .get(LATEST_RELEASE_API)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(network_error)?;
    match response.status() {
        StatusCode::NOT_FOUND => return Ok(no_release(current)),
        StatusCode::FORBIDDEN | StatusCode::TOO_MANY_REQUESTS => {
            return Err("错误：更新查询受限".into())
        }
        status if !status.is_success() => return Err("错误：更新服务暂不可用".into()),
        _ => {}
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_RESPONSE_BYTES as u64)
    {
        return Err("错误：更新信息过大".into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(network_error)? {
        if bytes.len() + chunk.len() > MAX_RESPONSE_BYTES {
            return Err("错误：更新信息过大".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    parse_release(current, &bytes)
}

fn network_error(error: reqwest::Error) -> String {
    if error.is_timeout() {
        "错误：网络连接超时"
    } else {
        "错误：无法连接服务器"
    }
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fixture(tag: &str) -> serde_json::Value {
        json!({"tag_name":tag,"html_url":format!("{REPOSITORY_URL}/releases/tag/{tag}"),
            "body":"更新说明","published_at":"2026-10-09T00:00:00Z","assets":[]})
    }
    fn parse(current: &str, release: &serde_json::Value) -> Result<UpdateInfo, String> {
        parse_release(current, &serde_json::to_vec(release).unwrap())
    }
    #[test]
    fn numeric_versions_never_offer_downgrades_and_ignore_unpublished_releases() {
        assert_eq!(
            parse("2.0.0", &fixture("v2.0.0")).unwrap().status,
            "up_to_date"
        );
        assert_eq!(
            parse("2.0.0", &fixture("v1.9.0")).unwrap().status,
            "up_to_date"
        );
        assert_eq!(
            parse("2.9.0", &fixture("v2.10.0")).unwrap().status,
            "update_available"
        );
        for (field, value) in [("draft", true), ("prerelease", true)] {
            let mut release = fixture("v3.0.0");
            release[field] = json!(value);
            assert_eq!(parse("2.0.0", &release).unwrap().status, "no_release");
        }
        assert_eq!(
            parse("2.0.0", &fixture("v3.0.0-beta.1")).unwrap().status,
            "no_release"
        );
    }
    #[test]
    fn download_links_belong_to_the_exact_repository_tag_and_windows_asset() {
        let mut release = fixture("v2.1.0");
        release["assets"] = json!([
            {"name":"DiscoAS_2.1.0_x64-setup.exe","browser_download_url":format!("{REPOSITORY_URL}/releases/download/v2.1.0/DiscoAS_2.1.0_x64-setup.exe")},
            {"name":"DiscoAS_2.1.0_x64-setup-full.exe","browser_download_url":"https://evil.example/installer.exe"},
            {"name":"DiscoAS_2.1.0_arm64-setup.exe","browser_download_url":"https://evil.example/other.exe"}
        ]);
        let info = parse("2.0.0", &release).unwrap();
        assert!(info.download_url.is_some());
        assert!(info.full_download_url.is_none());
        release["html_url"] =
            json!("https://github.com/caipeilan/DiscoAS.evil/releases/tag/v2.1.0");
        assert!(parse("2.0.0", &release).is_err());
    }
    #[test]
    fn malformed_or_oversized_updates_are_short_errors_and_notes_remain_plain_text() {
        assert!(parse("2.0.0", &fixture("not-a-version")).is_err());
        assert!(parse_release("2.0.0", b"not json").is_err());
        assert!(parse_release("2.0.0", &vec![b' '; MAX_RESPONSE_BYTES + 1]).is_err());
        let mut release = fixture("v2.0.1");
        release["body"] = json!("<script>example</script>".repeat(2000));
        let info = parse("2.0.0", &release).unwrap();
        assert_eq!(info.release_notes.chars().count(), 20_000);
        assert!(info.release_notes.starts_with("<script>"));
        assert_eq!(no_release("2.0.0").status, "no_release");
    }
}
