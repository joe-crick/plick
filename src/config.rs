//! User configuration for Plick.
//!
//! Handles loading and saving preferences such as output directory.
//! Config is stored at `~/.config/plick/config.toml`.
//!
//! Invalid configs cannot be constructed: `Fps` newtypes reject zero,
//! and `Config::new()` rejects relative output paths.

use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::types::Fps;

/// Recording and output preferences.
///
/// All invariants are enforced by the type system:
/// - `Fps` fields are guaranteed non-zero.
/// - `output_dir` is validated as absolute at construction.
/// - `max_duration_secs` is validated as non-zero at construction.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Config {
    pub output_dir: PathBuf,
    pub capture_fps: Fps,
    pub gif_fps: Fps,
    pub max_duration_secs: u64,
    pub countdown_secs: u64,
    /// Maximum GIF output width in pixels, preserving aspect ratio.
    /// `None` keeps the original resolution.
    #[serde(default = "default_gif_width")]
    pub gif_width: Option<u32>,
    /// Number of colors in the GIF palette (2–256).
    /// Lower values compress better; 128 is a good default for UI recordings.
    #[serde(default = "default_gif_colors")]
    pub gif_colors: u32,
}

fn default_gif_width() -> Option<u32> {
    Some(960)
}

fn default_gif_colors() -> u32 {
    128
}

impl Default for Config {
    fn default() -> Self {
        let output_dir = dirs::video_dir().unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("/tmp"))
                .join("Videos")
        });

        Self {
            output_dir,
            // SAFETY: literals are non-zero, so unwrap is fine.
            capture_fps: Fps::new(30).unwrap(),
            gif_fps: Fps::new(15).unwrap(),
            max_duration_secs: 120,
            countdown_secs: 3,
            gif_width: default_gif_width(),
            gif_colors: default_gif_colors(),
        }
    }
}

impl Config {
    /// Build a `Config` from explicit values, validating constraints that
    /// types alone don't cover (absolute path, non-zero duration).
    pub fn new(
        output_dir: PathBuf,
        capture_fps: Fps,
        gif_fps: Fps,
        max_duration_secs: u64,
        countdown_secs: u64,
        gif_width: Option<u32>,
        gif_colors: u32,
    ) -> Result<Self> {
        ensure!(
            output_dir.is_absolute(),
            "output_dir must be an absolute path, got: {}",
            output_dir.display()
        );
        ensure!(max_duration_secs > 0, "max_duration_secs must be > 0");

        Ok(Self {
            output_dir,
            capture_fps,
            gif_fps,
            max_duration_secs,
            countdown_secs,
            gif_width,
            gif_colors,
        })
    }

    /// Returns the path to the config file.
    fn config_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("plick")
            .join("config.toml")
    }

    /// Load configuration from disk, falling back to defaults.
    pub fn load() -> Self {
        let path = Self::config_path();
        if !path.exists() {
            return Self::default();
        }

        std::fs::read_to_string(&path)
            .ok()
            .and_then(|contents| toml::from_str(&contents).ok())
            .unwrap_or_default()
    }

    /// Save configuration to disk.
    pub fn save(&self) -> Result<()> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create config directory: {}", parent.display()))?;
        }
        let contents =
            toml::to_string_pretty(self).context("Failed to serialize config")?;
        std::fs::write(&path, contents)
            .with_context(|| format!("Failed to write config to {}", path.display()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_expected_values() {
        let c = Config::default();
        assert_eq!(c.capture_fps.get(), 30);
        assert_eq!(c.gif_fps.get(), 15);
        assert_eq!(c.max_duration_secs, 120);
        assert_eq!(c.countdown_secs, 3);
        assert_eq!(c.gif_width, Some(960));
        assert_eq!(c.gif_colors, 128);
        assert!(c.output_dir.is_absolute());
    }

    #[test]
    fn new_rejects_relative_path() {
        let result = Config::new(
            PathBuf::from("relative/path"),
            Fps::new(30).unwrap(),
            Fps::new(15).unwrap(),
            120,
            3,
            Some(960),
            128,
        );
        assert!(result.is_err());
    }

    #[test]
    fn new_rejects_zero_duration() {
        let result = Config::new(
            PathBuf::from("/tmp"),
            Fps::new(30).unwrap(),
            Fps::new(15).unwrap(),
            0,
            3,
            Some(960),
            128,
        );
        assert!(result.is_err());
    }

    #[test]
    fn new_accepts_valid_values() {
        let result = Config::new(
            PathBuf::from("/home/user/Videos"),
            Fps::new(30).unwrap(),
            Fps::new(15).unwrap(),
            120,
            3,
            Some(960),
            128,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn config_roundtrip_serialization() {
        let config = Config::default();
        let serialized = toml::to_string_pretty(&config).unwrap();
        let deserialized: Config = toml::from_str(&serialized).unwrap();
        assert_eq!(config, deserialized);
    }

    #[test]
    fn deserialization_rejects_zero_fps() {
        let bad_toml = r#"
            output_dir = "/tmp"
            capture_fps = 0
            gif_fps = 15
            max_duration_secs = 120
            countdown_secs = 3
        "#;
        let result: Result<Config, _> = toml::from_str(bad_toml);
        assert!(result.is_err());
    }
}
