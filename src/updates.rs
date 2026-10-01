//! Manual release discovery only. Downloads and installation stay in the browser.
use semver::Version;
use serde::Deserialize;
use std::process::Command;

const API_URL: &str = "https://api.github.com/repos/las6/local-haunt/releases/latest";

#[derive(Debug, PartialEq, Eq)]
pub enum Update {
    Current,
    Available { version: String, url: String },
    NoRelease,
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
}

// Called on a background thread, with bounded network time and no credentials.
pub fn check() -> Result<Update, String> {
    let output = Command::new("/usr/bin/curl")
        .args([
            "--silent",
            "--show-error",
            "--proto",
            "=https",
            "--connect-timeout",
            "5",
            "--max-time",
            "15",
            "--header",
            "Accept: application/vnd.github+json",
            "--user-agent",
            concat!("Local-Haunt/", env!("CARGO_PKG_VERSION")),
            "--write-out",
            "\n%{http_code}",
            API_URL,
        ])
        .output()
        .map_err(|_| "Could not start the update check. Try again.".to_owned())?;
    if !output.status.success() {
        return Err("Could not reach GitHub. Check your connection and try again.".into());
    }
    let response = String::from_utf8(output.stdout)
        .map_err(|_| "GitHub returned an unreadable response.".to_owned())?;
    let (body, status) = response
        .rsplit_once('\n')
        .ok_or("GitHub returned an incomplete response.")?;
    parse_response(body, status, env!("CARGO_PKG_VERSION"))
}

fn parse_response(body: &str, status: &str, installed: &str) -> Result<Update, String> {
    match status {
        "404" => return Ok(Update::NoRelease),
        "403" | "429" => {
            return Err("GitHub temporarily limited update checks. Try again later.".into());
        }
        "200" => {}
        _ => return Err("Could not check for updates. Try again later.".into()),
    }
    let release: Release = serde_json::from_str(body)
        .map_err(|_| "GitHub returned invalid release information.".to_owned())?;
    let version = Version::parse(
        release
            .tag_name
            .strip_prefix('v')
            .unwrap_or(&release.tag_name),
    )
    .map_err(|_| "The latest release has an unrecognized version.".to_owned())?;
    let installed =
        Version::parse(installed).map_err(|_| "The installed version is invalid.".to_owned())?;
    if release.draft || release.prerelease || !version.pre.is_empty() {
        return Err("No stable release was returned. Try again later.".into());
    }
    if version <= installed {
        return Ok(Update::Current);
    }
    if !release
        .assets
        .iter()
        .any(|asset| asset.name.ends_with("-macos-arm64.zip"))
    {
        return Err(
            "The latest release does not have an Apple Silicon download yet. Try again later."
                .into(),
        );
    }
    // Construct the destination from a validated tag, rather than trusting a response URL.
    Ok(Update::Available {
        version: version.to_string(),
        url: format!(
            "https://github.com/las6/local-haunt/releases/tag/{}",
            release.tag_name
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::{Update, parse_response};

    fn release(version: &str) -> String {
        format!(
            r#"{{"tag_name":"{version}","draft":false,"prerelease":false,"assets":[{{"name":"Local-Haunt-{version}-macos-arm64.zip"}}]}}"#
        )
    }

    #[test]
    fn compares_versions_numerically_and_accepts_optional_v_prefix() {
        for tag in ["v0.1.10", "0.1.10"] {
            assert!(
                matches!(parse_response(&release(tag), "200", "0.1.9"), Ok(Update::Available { version, .. }) if version == "0.1.10")
            );
        }
        for tag in ["v0.1.9", "v0.1.8"] {
            assert_eq!(
                parse_response(&release(tag), "200", "0.1.9"),
                Ok(Update::Current)
            );
        }
    }

    #[test]
    fn handles_missing_release_failures_and_incomplete_downloads() {
        assert_eq!(parse_response("{}", "404", "0.1.1"), Ok(Update::NoRelease));
        for status in ["403", "429", "500"] {
            assert!(parse_response("{}", status, "0.1.1").is_err());
        }
        assert!(parse_response("broken", "200", "0.1.1").is_err());
        assert!(parse_response(&release("not-a-version"), "200", "0.1.1").is_err());
        assert!(parse_response(&release("v0.2.0-beta.1"), "200", "0.1.1").is_err());
        assert!(
            parse_response(
                &release("v0.2.0").replace("-macos-arm64.zip", "-intel.zip"),
                "200",
                "0.1.1"
            )
            .is_err()
        );
        assert!(
            parse_response(
                &release("v0.2.0").replace("\"draft\":false", "\"draft\":true"),
                "200",
                "0.1.1"
            )
            .is_err()
        );
    }
}
