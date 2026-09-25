//! Hosting platforms' own UI modules (HLIN-A-0014, specification HLIN-S-0007).
//!
//! A platform ships its UI as a module; the shell serves it from its own origin
//! under `/m/`, runs it in a sandboxed frame, and carries its requests under
//! `/p/`. This module holds what those share: the limits every module runs
//! within.

pub mod assets;
pub mod changes;

use serde::{Deserialize, Serialize};

pub mod requests;

/// The bounds a module runs within (HLIN-S-0007, *Limits*).
///
/// Operator configuration, because the right numbers depend on what the
/// platforms behind a shell do: a platform exporting reports needs a larger
/// response than one ticking boxes, and the operator is the one who knows
/// which is which. Every value has the specification's default, so a shell
/// that never mentions modules gets sensible bounds without saying so.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ModuleLimits {
    /// The largest entry document the shell will serve.
    pub entry_bytes: usize,
    /// The largest of any other asset.
    pub asset_bytes: usize,
    /// The largest request body a module may send.
    pub request_bytes: usize,
    /// The largest whole response body passed back to a module. Streamed
    /// responses are bounded by rate and idleness instead.
    pub response_bytes: usize,
    /// Requests one frame may have in flight, streams included.
    pub fetches_in_flight: u32,
    /// Messages one frame may send per second, stream credit excluded.
    pub messages_per_second: u32,
    /// Streamed responses one frame may hold open.
    pub streams: u32,
    /// How fast one stream may deliver, in bytes per second.
    pub stream_bytes_per_second: usize,
    /// How long a stream may go without a byte before the shell ends it.
    pub stream_idle_seconds: u64,
    /// The largest state a module may hand back before it is unmounted.
    pub state_bytes: usize,
}

const KIB: usize = 1024;
const MIB: usize = 1024 * KIB;

impl Default for ModuleLimits {
    fn default() -> Self {
        Self {
            entry_bytes: 256 * KIB,
            asset_bytes: 16 * MIB,
            request_bytes: MIB,
            response_bytes: 4 * MIB,
            fetches_in_flight: 8,
            messages_per_second: 50,
            streams: 2,
            stream_bytes_per_second: MIB,
            stream_idle_seconds: 60,
            state_bytes: 64 * KIB,
        }
    }
}

impl ModuleLimits {
    /// These limits, with a platform's overrides applied.
    pub fn with(self, overrides: &LimitOverrides) -> Self {
        Self {
            entry_bytes: overrides.entry_bytes.unwrap_or(self.entry_bytes),
            asset_bytes: overrides.asset_bytes.unwrap_or(self.asset_bytes),
            request_bytes: overrides.request_bytes.unwrap_or(self.request_bytes),
            response_bytes: overrides.response_bytes.unwrap_or(self.response_bytes),
            fetches_in_flight: overrides
                .fetches_in_flight
                .unwrap_or(self.fetches_in_flight),
            messages_per_second: overrides
                .messages_per_second
                .unwrap_or(self.messages_per_second),
            streams: overrides.streams.unwrap_or(self.streams),
            stream_bytes_per_second: overrides
                .stream_bytes_per_second
                .unwrap_or(self.stream_bytes_per_second),
            stream_idle_seconds: overrides
                .stream_idle_seconds
                .unwrap_or(self.stream_idle_seconds),
            state_bytes: overrides.state_bytes.unwrap_or(self.state_bytes),
        }
    }

    /// Whether these limits can be run with.
    ///
    /// Checked at startup. A zero anywhere makes every module fail in a way
    /// that looks like the module's fault, and a byte limit under a kilobyte
    /// is a typo for one in kilobytes far more often than it is a decision.
    /// Streams may be zero: that is how an operator turns streaming off.
    pub fn check(&self) -> Result<(), String> {
        let bytes = [
            ("entry_bytes", self.entry_bytes),
            ("asset_bytes", self.asset_bytes),
            ("request_bytes", self.request_bytes),
            ("response_bytes", self.response_bytes),
            ("stream_bytes_per_second", self.stream_bytes_per_second),
            ("state_bytes", self.state_bytes),
        ];
        for (name, value) in bytes {
            if value < KIB {
                return Err(format!(
                    "modules.limits.{name} is {value}, under the floor of {KIB}; \
                     it is counted in bytes"
                ));
            }
        }

        let counts = [
            ("fetches_in_flight", self.fetches_in_flight),
            ("messages_per_second", self.messages_per_second),
        ];
        for (name, value) in counts {
            if value == 0 {
                return Err(format!(
                    "modules.limits.{name} is 0, which lets no module do anything"
                ));
            }
        }

        if self.stream_idle_seconds == 0 {
            return Err(
                "modules.limits.stream_idle_seconds is 0, which ends every stream at once"
                    .to_string(),
            );
        }

        if self.streams > self.fetches_in_flight {
            return Err(format!(
                "modules.limits.streams ({}) is more than fetches_in_flight ({}), \
                 which counts streams too",
                self.streams, self.fetches_in_flight
            ));
        }

        Ok(())
    }
}

/// A platform's departures from the shell's module limits.
///
/// Every field optional: a platform names only what it needs to be different,
/// and anything it leaves out follows the shell's `[modules.limits]`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct LimitOverrides {
    /// See [`ModuleLimits::entry_bytes`].
    pub entry_bytes: Option<usize>,
    /// See [`ModuleLimits::asset_bytes`].
    pub asset_bytes: Option<usize>,
    /// See [`ModuleLimits::request_bytes`].
    pub request_bytes: Option<usize>,
    /// See [`ModuleLimits::response_bytes`].
    pub response_bytes: Option<usize>,
    /// See [`ModuleLimits::fetches_in_flight`].
    pub fetches_in_flight: Option<u32>,
    /// See [`ModuleLimits::messages_per_second`].
    pub messages_per_second: Option<u32>,
    /// See [`ModuleLimits::streams`].
    pub streams: Option<u32>,
    /// See [`ModuleLimits::stream_bytes_per_second`].
    pub stream_bytes_per_second: Option<usize>,
    /// See [`ModuleLimits::stream_idle_seconds`].
    pub stream_idle_seconds: Option<u64>,
    /// See [`ModuleLimits::state_bytes`].
    pub state_bytes: Option<usize>,
}

/// The shell-wide `[modules]` table.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ModulesConfig {
    /// The limits every platform's modules run within, unless it overrides
    /// them.
    pub limits: ModuleLimits,
}

/// A platform's own `modules` table.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct PlatformModules {
    /// Where this platform's limits differ from the shell's.
    pub limits: LimitOverrides,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_are_the_ones_the_specification_names() {
        let limits = ModuleLimits::default();
        assert_eq!(limits.entry_bytes, 256 * 1024);
        assert_eq!(limits.asset_bytes, 16 * 1024 * 1024);
        assert_eq!(limits.request_bytes, 1024 * 1024);
        assert_eq!(limits.response_bytes, 4 * 1024 * 1024);
        assert_eq!(limits.fetches_in_flight, 8);
        assert_eq!(limits.messages_per_second, 50);
        assert_eq!(limits.streams, 2);
        assert_eq!(limits.stream_bytes_per_second, 1024 * 1024);
        assert_eq!(limits.stream_idle_seconds, 60);
        assert_eq!(limits.state_bytes, 64 * 1024);
        assert!(limits.check().is_ok());
    }

    #[test]
    fn a_platform_changes_only_what_it_names() {
        let overrides = LimitOverrides {
            response_bytes: Some(32 * 1024 * 1024),
            streams: Some(0),
            ..Default::default()
        };
        let limits = ModuleLimits::default().with(&overrides);
        assert_eq!(limits.response_bytes, 32 * 1024 * 1024);
        assert_eq!(limits.streams, 0);
        assert_eq!(limits.request_bytes, ModuleLimits::default().request_bytes);
    }

    #[test]
    fn limits_nobody_could_run_with_are_refused() {
        let refused = |limits: ModuleLimits| limits.check().expect_err("should be refused");

        assert!(
            refused(ModuleLimits {
                request_bytes: 1,
                ..Default::default()
            })
            .contains("request_bytes")
        );
        assert!(
            refused(ModuleLimits {
                fetches_in_flight: 0,
                ..Default::default()
            })
            .contains("fetches_in_flight")
        );
        assert!(
            refused(ModuleLimits {
                stream_idle_seconds: 0,
                ..Default::default()
            })
            .contains("stream_idle_seconds")
        );
        assert!(
            refused(ModuleLimits {
                streams: 9,
                ..Default::default()
            })
            .contains("streams")
        );
    }

    #[test]
    fn streaming_can_be_turned_off() {
        let limits = ModuleLimits {
            streams: 0,
            ..Default::default()
        };
        assert!(limits.check().is_ok());
    }

    #[test]
    fn a_table_that_names_nothing_is_the_defaults() {
        let config: ModulesConfig = toml::from_str("").unwrap();
        assert_eq!(config.limits, ModuleLimits::default());

        let config: ModulesConfig = toml::from_str("[limits]\nstreams = 1\n").unwrap();
        assert_eq!(config.limits.streams, 1);
        assert_eq!(
            config.limits.request_bytes,
            ModuleLimits::default().request_bytes
        );
    }

    #[test]
    fn a_misspelt_limit_is_refused_rather_than_ignored() {
        assert!(toml::from_str::<ModulesConfig>("[limits]\nrequest_byte = 5\n").is_err());
    }
}
