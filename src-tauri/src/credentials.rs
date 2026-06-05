use std::path::PathBuf;

/// Claude Code credential file locations, in priority order
/// (mirrors Clawdmeter's Windows daemon fallback logic).
fn candidate_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(home) = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")) {
        paths.push(PathBuf::from(&home).join(".claude").join(".credentials.json"));
    }
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        paths.push(PathBuf::from(&local).join("Claude").join(".credentials.json"));
    }
    if let Some(roaming) = std::env::var_os("APPDATA") {
        paths.push(PathBuf::from(&roaming).join("Claude").join(".credentials.json"));
    }
    paths
}

pub struct Credentials {
    pub token: String,
    /// `expiresAt` from the file, ms epoch. Informational only — the request
    /// is always attempted; the server is the authority on validity.
    pub expires_at: Option<i64>,
}

impl Credentials {
    pub fn looks_expired(&self) -> bool {
        self.expires_at.is_some_and(|t| t < crate::usage::now_ms())
    }
}

/// Parse a credentials blob. Supports both
/// `{"claudeAiOauth": {"accessToken": ...}}` and a bare `{"accessToken": ...}` layout.
fn parse(text: &str) -> Option<Credentials> {
    let json = serde_json::from_str::<serde_json::Value>(text).ok()?;
    let oauth = json.get("claudeAiOauth").unwrap_or(&json);
    let token = oauth.get("accessToken").and_then(|v| v.as_str())?;
    Some(Credentials {
        token: token.to_string(),
        expires_at: oauth.get("expiresAt").and_then(|v| v.as_i64()),
    })
}

/// On macOS, Claude Code stores its OAuth credentials in the login keychain
/// (service `Claude Code-credentials`) rather than in a file. Read them via the
/// `security` CLI so we avoid pulling in a keychain crate.
#[cfg(target_os = "macos")]
fn read_keychain() -> Option<Credentials> {
    let output = std::process::Command::new("/usr/bin/security")
        .args(["find-generic-password", "-s", "Claude Code-credentials", "-w"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    parse(String::from_utf8(output.stdout).ok()?.trim())
}

/// Read the OAuth credentials, fresh on every call so a refresh by Claude
/// Code (or a re-login) is picked up on the next poll.
pub fn read() -> Result<Credentials, String> {
    // macOS keeps credentials in the keychain by default; the file locations
    // below remain the fallback (Linux/Windows, or keychain opted out).
    #[cfg(target_os = "macos")]
    if let Some(creds) = read_keychain() {
        return Ok(creds);
    }

    for path in candidate_paths() {
        let Ok(text) = std::fs::read_to_string(&path) else { continue };
        if let Some(creds) = parse(&text) {
            return Ok(creds);
        }
    }
    Err("no Claude credentials found — run `claude login`".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage::now_ms;

    #[test]
    fn parses_nested_claude_ai_oauth_layout() {
        let creds = parse(
            r#"{"claudeAiOauth": {"accessToken": "tok-123", "expiresAt": 1780682400000}}"#,
        )
        .unwrap();
        assert_eq!(creds.token, "tok-123");
        assert_eq!(creds.expires_at, Some(1_780_682_400_000));
    }

    #[test]
    fn parses_bare_access_token_layout() {
        let creds = parse(r#"{"accessToken": "tok-456"}"#).unwrap();
        assert_eq!(creds.token, "tok-456");
        assert_eq!(creds.expires_at, None);
    }

    #[test]
    fn returns_none_without_an_access_token() {
        assert!(parse(r#"{"claudeAiOauth": {"refreshToken": "x"}}"#).is_none());
        assert!(parse(r#"{"somethingElse": true}"#).is_none());
    }

    #[test]
    fn returns_none_for_invalid_json() {
        assert!(parse("not json at all").is_none());
        assert!(parse("").is_none());
    }

    #[test]
    fn looks_expired_is_false_for_future_expiry() {
        let creds = Credentials { token: "t".into(), expires_at: Some(now_ms() + 60_000) };
        assert!(!creds.looks_expired());
    }

    #[test]
    fn looks_expired_is_true_for_past_expiry() {
        let creds = Credentials { token: "t".into(), expires_at: Some(now_ms() - 60_000) };
        assert!(creds.looks_expired());
    }

    #[test]
    fn looks_expired_is_false_when_expiry_is_unknown() {
        let creds = Credentials { token: "t".into(), expires_at: None };
        assert!(!creds.looks_expired());
    }
}
