// SPDX-License-Identifier: LGPL-3.0-only WITH LGPL-3.0-linking-exception
//! Worker configuration parsed from the page query string.
//!
//! - `?api=<base>`: API base URL, same rules as ActionRunner-web: a
//!   same-origin absolute path (`/console-api`), `http://localhost…` /
//!   `http://127.0.0.1…` for development, or a URL on the page's own origin.
//!   Anything else is ignored and the base stays empty (same origin).
//! - `?scenario=<name>`: replay a fixture scenario instead of live data.
//!   `live` (or no parameter) selects live data.

use alloc::string::String;
use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

/// Name of the default (non-fixture) scenario.
pub const LIVE_SCENARIO: &str = "live";

/// Settings the UI forwards to the worker in [`crate::UiCommand::Configure`].
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct WorkerConfig {
    /// API base without a trailing slash; empty means same origin.
    pub api_base: String,
    /// Fixture scenario name; `None` means live data.
    pub scenario: Option<String>,
}

impl WorkerConfig {
    /// Parse `location.search` (with or without the leading `?`).
    ///
    /// `origin` is `location.origin` and is used to accept absolute API URLs
    /// on the page's own origin.
    #[must_use]
    pub fn from_query(search: &str, origin: &str) -> Self {
        let mut cfg = Self::default();
        for pair in search.trim_start_matches('?').split('&') {
            let Some((key, raw)) = pair.split_once('=') else {
                continue;
            };
            let value = percent_decode(raw);
            match key {
                "api" => {
                    let trimmed = value.trim_end_matches('/');
                    if is_safe_api_base(trimmed, origin) {
                        cfg.api_base = trimmed.into();
                    }
                }
                "scenario" => {
                    let name = value.trim();
                    cfg.scenario = if name.is_empty() || name == LIVE_SCENARIO {
                        None
                    } else {
                        Some(name.into())
                    };
                }
                _ => {}
            }
        }
        cfg
    }

    /// Join the API base and an absolute path (`/api/repos`).
    #[must_use]
    pub fn url(&self, path: &str) -> String {
        let mut s = String::with_capacity(self.api_base.len() + path.len());
        s.push_str(&self.api_base);
        s.push_str(path);
        s
    }
}

fn is_safe_api_base(base: &str, origin: &str) -> bool {
    if base.is_empty() {
        return false;
    }
    (base.starts_with('/') && !base.starts_with("//"))
        || base.starts_with("http://127.0.0.1")
        || base.starts_with("http://localhost")
        || (!origin.is_empty() && is_same_origin(base, origin))
}

fn is_same_origin(base: &str, origin: &str) -> bool {
    base == origin
        || base
            .strip_prefix(origin)
            .is_some_and(|rest| rest.starts_with('/'))
}

/// Decode `%XX` escapes and `+` (as space). Invalid escapes are kept as is.
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i: usize = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => match (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                (Some(h), Some(l)) => {
                    out.push(h << 4 | l);
                    i += 3;
                }
                _ => {
                    out.push(b'%');
                    i += 1;
                }
            },
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ORIGIN: &str = "https://console.example.org";

    #[test]
    fn defaults_to_same_origin_live() {
        assert_eq!(
            WorkerConfig::from_query("", ORIGIN),
            WorkerConfig::default()
        );
        assert_eq!(
            WorkerConfig::from_query("?scenario=live", ORIGIN),
            WorkerConfig::default()
        );
    }

    #[test]
    fn parses_api_and_scenario() {
        let cfg = WorkerConfig::from_query("?api=/edge/&scenario=node-lost&x=1", ORIGIN);
        assert_eq!(cfg.api_base, "/edge");
        assert_eq!(cfg.scenario.as_deref(), Some("node-lost"));
        assert_eq!(cfg.url("/api/repos"), "/edge/api/repos");
    }

    #[test]
    fn accepts_dev_and_same_origin_hosts() {
        for ok in [
            "http://127.0.0.1:8000",
            "http://localhost:8000/",
            "https://console.example.org/api-proxy",
            "http%3A%2F%2Flocalhost%3A9000",
        ] {
            let cfg = WorkerConfig::from_query(&alloc::format!("api={ok}"), ORIGIN);
            assert!(!cfg.api_base.is_empty(), "{ok} rejected");
        }
    }

    #[test]
    fn rejects_foreign_hosts() {
        for bad in [
            "https://evil.example",
            "//evil.example",
            "https://console.example.org.evil.example",
            "javascript:alert(1)",
        ] {
            let cfg = WorkerConfig::from_query(&alloc::format!("api={bad}"), ORIGIN);
            assert_eq!(cfg.api_base, "", "{bad} accepted");
        }
    }

    #[test]
    fn percent_decoding_is_lenient() {
        assert_eq!(percent_decode("a%2Fb+c"), "a/b c");
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("%zz%4"), "%zz%4");
    }
}
