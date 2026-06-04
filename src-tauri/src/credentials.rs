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

/// Read the OAuth credentials, fresh on every call so a refresh by Claude
/// Code (or a re-login) is picked up on the next poll. Supports both
/// `{"claudeAiOauth": {"accessToken": ...}}` and a bare `{"accessToken": ...}` layout.
pub fn read() -> Result<Credentials, String> {
    for path in candidate_paths() {
        let Ok(text) = std::fs::read_to_string(&path) else { continue };
        let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else { continue };
        let oauth = json.get("claudeAiOauth").unwrap_or(&json);
        let Some(token) = oauth.get("accessToken").and_then(|v| v.as_str()) else { continue };
        return Ok(Credentials {
            token: token.to_string(),
            expires_at: oauth.get("expiresAt").and_then(|v| v.as_i64()),
        });
    }
    Err("no Claude credentials found — run `claude login`".into())
}
