//! Aurora Dark semantic tokens + defensive classifiers — generic, pure logic
//! with no framework dependency. This is the general core shared by every
//! Colliery project; the higher-level state vocab (graph-health/reactor terms)
//! lives alongside the data-display widgets in the `widgets` module.

/// Semantic palette (matches `tokens.ts` TOKEN and the CSS custom properties).
pub mod token {
    pub const ICE: &str = "#7fb2ff";
    pub const TEAL: &str = "#5fd0c5";
    pub const VIOLET: &str = "#9d8cff";
    pub const GOLD: &str = "#d8a657";
    pub const OK: &str = "#4bd07f";
    pub const BAD: &str = "#f06464";
    pub const SKIP: &str = "#cf83a4";
    pub const MUTED: &str = "#8b95a3";
    pub const FAINT: &str = "#5b6573";
}

/// Execution / task status → color. Case-insensitive, muted fallback
/// (REQ-007 defensive rendering — server strings are not a fixed enum).
pub fn status_color(status: &str) -> &'static str {
    match status.to_lowercase().as_str() {
        "running" => token::ICE,
        "completed" => token::OK,
        "failed" => token::BAD,
        "scheduled" => token::VIOLET,
        "pending" | "paused" => token::MUTED,
        "cancelled" | "canceled" => token::GOLD,
        "skipped" => token::SKIP,
        _ => token::MUTED,
    }
}

/// Tinted pill/badge background: the status color at `1c` alpha (spec §Pills).
/// `#7fb2ff` → `#7fb2ff1c`.
pub fn pill_bg(hex: &str) -> String {
    format!("{hex}1c")
}

/// Classified error kind → UI presentation (ported from `errors.ts`).
pub struct Classified {
    pub title: &'static str,
    pub message: String,
    pub code: Option<String>,
    pub retryable: bool,
    /// alert accent color var name (`--bad` or `--gold`)
    pub color_var: &'static str,
}

/// A transport/HTTP error shape for `classify`. Apps map their own error type
/// into this (HTTP status + optional server code/message, or a network failure).
#[derive(Clone, PartialEq)]
pub enum ApiError {
    /// Carries an HTTP status + optional server code/message.
    Http { status: u16, message: String, code: Option<String> },
    /// Transport failure (server unreachable).
    Network,
    Unknown(String),
}

pub fn classify(err: &ApiError) -> Classified {
    match err {
        ApiError::Http { status, message, code } => {
            let (kind_title, color_var, retryable) = match status {
                401 | 403 => ("Not authorized", "--bad", false),
                404 => ("Not found", "--bad", false),
                400 | 422 => ("Invalid request", "--gold", false),
                s if *s >= 500 => ("Something went wrong", "--bad", true),
                _ => ("Something went wrong", "--bad", false),
            };
            Classified {
                title: kind_title,
                message: message.clone(),
                code: code.clone(),
                retryable,
                color_var,
            }
        }
        ApiError::Network => Classified {
            title: "Cannot reach server",
            message: "Could not reach the server. Check the URL and that CORS is enabled.".into(),
            code: None,
            retryable: true,
            color_var: "--bad",
        },
        ApiError::Unknown(m) => Classified {
            title: "Something went wrong",
            message: m.clone(),
            code: None,
            retryable: false,
            color_var: "--bad",
        },
    }
}
