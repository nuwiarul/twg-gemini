use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub fn load_audio_output() -> bool {
    std::fs::read_to_string(resolve_settings_path().with_file_name("audio-output.json"))
        .ok()
        .and_then(|v| serde_json::from_str(&v).ok())
        .unwrap_or(true)
}

pub fn save_audio_output(enabled: bool) -> Result<(), String> {
    let path = resolve_settings_path().with_file_name("audio-output.json");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, if enabled { "true" } else { "false" })
        .map_err(|e| format!("Gagal menyimpan pengaturan suara: {e}"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ServerProtocol {
    #[default]
    Http,
    Https,
}

impl ServerProtocol {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::Https => "https",
        }
    }

    pub fn default_port(self) -> u16 {
        match self {
            Self::Http => 80,
            Self::Https => 443,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalServerSettings {
    pub protocol: ServerProtocol,
    pub host: String,
    /// Optional port string. Empty means protocol default (80 for HTTP, 443 for HTTPS).
    #[serde(default)]
    pub port: String,
}

impl Default for LocalServerSettings {
    fn default() -> Self {
        Self {
            protocol: ServerProtocol::Http,
            host: "127.0.0.1".to_string(),
            port: "8010".to_string(),
        }
    }
}

impl LocalServerSettings {
    /// Parses a full URL (or bare `host[:port]`) into protocol, host, and port fields.
    pub fn from_url(raw: &str) -> Self {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Self::default();
        }
        let candidate = if trimmed.contains("://") {
            trimmed.to_string()
        } else {
            format!("http://{trimmed}")
        };

        if let Ok(parsed) = url::Url::parse(&candidate) {
            let protocol = match parsed.scheme().to_ascii_lowercase().as_str() {
                "https" | "wss" => ServerProtocol::Https,
                _ => ServerProtocol::Http,
            };
            if let Some(host_str) = parsed.host_str() {
                let port = parsed.port().map(|p| p.to_string()).unwrap_or_default();
                return Self {
                    protocol,
                    host: host_str.to_string(),
                    port,
                };
            }
        }

        Self::default()
    }

    /// If the user pasted a full URL (e.g. `https://twg.example.com:8443`) into the Host box,
    /// automatically splits it into `protocol`, `host`, and `port`.
    pub fn normalize_pasted_host(&mut self) {
        let trimmed = self.host.trim();
        if trimmed.starts_with("http://")
            || trimmed.starts_with("https://")
            || trimmed.starts_with("ws://")
            || trimmed.starts_with("wss://")
        {
            let parsed = Self::from_url(trimmed);
            self.protocol = parsed.protocol;
            self.host = parsed.host;
            if !parsed.port.is_empty() {
                self.port = parsed.port;
            } else {
                self.port.clear();
            }
        }
    }

    /// Builds a normalized `http(s)://host[:port]` URL after validating host & port.
    pub fn build_url(&self) -> Result<String, String> {
        let raw_host = self.host.trim().trim_end_matches('/');
        if raw_host.is_empty() {
            return Err("Host server tidak boleh kosong.".into());
        }
        if raw_host.contains(char::is_whitespace) || raw_host.contains('/') {
            return Err(
                "Format host tidak valid (isi domain atau IP saja, tanpa spasi atau path).".into(),
            );
        }

        // Format IPv6 literal with brackets if needed
        let host_formatted = if raw_host.contains(':')
            && !(raw_host.starts_with('[') && raw_host.ends_with(']'))
        {
            format!("[{raw_host}]")
        } else {
            raw_host.to_string()
        };

        let port_str = self.port.trim();
        let port_opt = if port_str.is_empty() {
            None
        } else {
            let p: u16 = port_str.parse().map_err(|_| {
                "Port harus berupa angka 1 - 65535 (atau kosongkan untuk port default).".to_string()
            })?;
            if p == 0 {
                return Err("Port harus berada di antara 1 - 65535.".into());
            }
            if p == self.protocol.default_port() {
                None
            } else {
                Some(p)
            }
        };

        let url_str = match port_opt {
            Some(p) => format!("{}://{host_formatted}:{p}", self.protocol.as_str()),
            None => format!("{}://{host_formatted}", self.protocol.as_str()),
        };

        // Final structural check with `url::Url`
        url::Url::parse(&url_str)
            .map_err(|_| "Alamat server tidak valid. Periksa kembali host dan port.".to_string())?;

        Ok(url_str)
    }
}

/// Returns the default server settings from `TWG_API_URL` (when set by launcher/env) or localhost.
pub fn default_server_settings() -> LocalServerSettings {
    std::env::var("TWG_API_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(|u| LocalServerSettings::from_url(&u))
        .unwrap_or_default()
}

/// Per-computer, per-user writable path for storing local server settings across Windows, macOS, and Linux.
pub fn resolve_settings_path() -> PathBuf {
    if let Ok(dir) = std::env::var("TWG_CONFIG_DIR")
        && !dir.trim().is_empty()
    {
        return PathBuf::from(dir).join("server_settings.json");
    }

    #[cfg(target_os = "windows")]
    {
        if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
            return PathBuf::from(local_app_data)
                .join("TWG-3D")
                .join("server_settings.json");
        }
        if let Some(app_data) = std::env::var_os("APPDATA") {
            return PathBuf::from(app_data)
                .join("TWG-3D")
                .join("server_settings.json");
        }
    }

    #[cfg(target_os = "macos")]
    {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join("TWG-3D")
                .join("server_settings.json");
        }
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME")
            && !xdg.is_empty()
        {
            return PathBuf::from(xdg)
                .join("twg-app")
                .join("server_settings.json");
        }
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home)
                .join(".config")
                .join("twg-app")
                .join("server_settings.json");
        }
    }

    PathBuf::from("twg_server_settings.json")
}

/// Loads saved local server settings if present; otherwise falls back to `default_server_settings()`.
/// Returns `(settings, loaded_from_local_file)`.
pub fn load_local_server_settings() -> (LocalServerSettings, bool) {
    let path = resolve_settings_path();
    load_local_server_settings_from(&path)
}

fn load_local_server_settings_from(path: &Path) -> (LocalServerSettings, bool) {
    if let Ok(raw) = std::fs::read_to_string(path)
        && let Ok(parsed) = serde_json::from_str::<LocalServerSettings>(&raw)
        && parsed.build_url().is_ok()
    {
        return (parsed, true);
    }
    (default_server_settings(), false)
}

/// Validates and persists `settings` to the local computer's settings file.
pub fn save_local_server_settings(settings: &LocalServerSettings) -> Result<PathBuf, String> {
    let path = resolve_settings_path();
    save_local_server_settings_to(&path, settings)?;
    Ok(path)
}

fn save_local_server_settings_to(
    path: &Path,
    settings: &LocalServerSettings,
) -> Result<(), String> {
    settings.build_url()?;
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Gagal membuat folder pengaturan lokal: {e}"))?;
    }
    let json = serde_json::to_string_pretty(settings)
        .map_err(|e| format!("Gagal serialisasi pengaturan: {e}"))?;
    std::fs::write(path, json).map_err(|e| format!("Gagal menyimpan pengaturan lokal: {e}"))
}

/// Removes the saved local override file (if any) and returns the default settings.
pub fn reset_local_server_settings() -> Result<LocalServerSettings, String> {
    let path = resolve_settings_path();
    if path.exists() {
        std::fs::remove_file(&path)
            .map_err(|e| format!("Gagal menghapus pengaturan lokal: {e}"))?;
    }
    Ok(default_server_settings())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_url_handles_http_https_custom_and_default_ports() {
        let s1 = LocalServerSettings {
            protocol: ServerProtocol::Http,
            host: "192.168.1.50".into(),
            port: "8010".into(),
        };
        assert_eq!(s1.build_url().unwrap(), "http://192.168.1.50:8010");

        let s2 = LocalServerSettings {
            protocol: ServerProtocol::Https,
            host: "twg.polri.go.id".into(),
            port: String::new(),
        };
        assert_eq!(s2.build_url().unwrap(), "https://twg.polri.go.id");

        let s3 = LocalServerSettings {
            protocol: ServerProtocol::Https,
            host: "twg.polri.go.id".into(),
            port: "443".into(),
        };
        assert_eq!(s3.build_url().unwrap(), "https://twg.polri.go.id");

        let s4 = LocalServerSettings {
            protocol: ServerProtocol::Https,
            host: "10.0.0.5".into(),
            port: "8443".into(),
        };
        assert_eq!(s4.build_url().unwrap(), "https://10.0.0.5:8443");

        let bad_port = LocalServerSettings {
            protocol: ServerProtocol::Http,
            host: "127.0.0.1".into(),
            port: "abc".into(),
        };
        assert!(bad_port.build_url().is_err());

        let zero_port = LocalServerSettings {
            protocol: ServerProtocol::Http,
            host: "127.0.0.1".into(),
            port: "0".into(),
        };
        assert!(zero_port.build_url().is_err());
    }

    #[test]
    fn from_url_and_paste_normalization_split_scheme_host_port() {
        let mut s = LocalServerSettings {
            host: "https://tactical.example.id:9443/".into(),
            ..Default::default()
        };
        s.normalize_pasted_host();
        assert_eq!(s.protocol, ServerProtocol::Https);
        assert_eq!(s.host, "tactical.example.id");
        assert_eq!(s.port, "9443");
        assert_eq!(s.build_url().unwrap(), "https://tactical.example.id:9443");

        let s_default = LocalServerSettings::from_url("https://twg.example.id");
        assert_eq!(s_default.protocol, ServerProtocol::Https);
        assert_eq!(s_default.host, "twg.example.id");
        assert_eq!(s_default.port, "");
    }

    #[test]
    fn save_and_load_local_settings_roundtrip() {
        let tmp = std::env::temp_dir().join(format!(
            "twg_settings_test_{}.json",
            uuid::Uuid::new_v4().simple()
        ));
        let orig = LocalServerSettings {
            protocol: ServerProtocol::Https,
            host: "command.local.lan".into(),
            port: "8443".into(),
        };
        save_local_server_settings_to(&tmp, &orig).unwrap();
        let (loaded, from_file) = load_local_server_settings_from(&tmp);
        let _ = std::fs::remove_file(&tmp);
        assert!(from_file);
        assert_eq!(loaded, orig);
    }
}
