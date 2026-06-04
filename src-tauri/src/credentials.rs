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
