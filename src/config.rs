//! Configuration: TOML loading, validation, defaults, CLI-merge helpers.

use crate::mapping::{Curve, MappingConfig, Stick};
use serde::Deserialize;
use std::path::Path;

/// Raw on-disk / TOML shape of the config (all optional so we can apply
/// defaults and let CLI overrides win).
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct FileConfig {
    pub stick: Option<String>,
    pub sensitivity: Option<f64>,
    pub decay: Option<f64>,
    pub deadzone: Option<f64>,
    pub invert_x: Option<bool>,
    pub invert_y: Option<bool>,
    pub curve: Option<String>,
    pub mouse_left_button: Option<String>,
    pub mouse_right_button: Option<String>,
    pub wheel_up: Option<String>,
    pub wheel_down: Option<String>,
}

/// Fully resolved runtime configuration.
#[derive(Debug, Clone)]
pub struct Config {
    pub mapping: MappingConfig,
    pub mouse_left_button: Option<X360Button>,
    pub mouse_right_button: Option<X360Button>,
    pub wheel_up: Option<X360Button>,
    pub wheel_down: Option<X360Button>,
}

/// X360 button bitmask values (XUSB_BUTTON).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum X360Button {
    DPadUp,
    DPadDown,
    DPadLeft,
    DPadRight,
    Start,
    Back,
    Ls,
    Rs,
    Lb,
    Rb,
    Guide,
    A,
    B,
    X,
    Y,
}

impl X360Button {
    /// Bitmask used in the wButtons field.
    pub fn mask(self) -> u16 {
        use X360Button::*;
        match self {
            DPadUp => 0x0001,
            DPadDown => 0x0002,
            DPadLeft => 0x0004,
            DPadRight => 0x0008,
            Start => 0x0010,
            Back => 0x0020,
            Ls => 0x0040,
            Rs => 0x0080,
            Lb => 0x0100,
            Rb => 0x0200,
            Guide => 0x0400,
            A => 0x1000,
            B => 0x2000,
            X => 0x4000,
            Y => 0x8000,
        }
    }

    fn parse(s: &str) -> Option<Self> {
        use X360Button::*;
        match s.to_ascii_lowercase().as_str() {
            "dpad_up" | "up" => Some(DPadUp),
            "dpad_down" | "down" => Some(DPadDown),
            "dpad_left" | "left" => Some(DPadLeft),
            "dpad_right" | "right" => Some(DPadRight),
            "start" => Some(Start),
            "back" => Some(Back),
            "ls" => Some(Ls),
            "rs" => Some(Rs),
            "lb" => Some(Lb),
            "rb" => Some(Rb),
            "guide" => Some(Guide),
            "a" => Some(A),
            "b" => Some(B),
            "x" => Some(X),
            "y" => Some(Y),
            _ => None,
        }
    }
}

fn parse_stick(s: &str) -> Result<Stick, String> {
    match s.to_ascii_lowercase().as_str() {
        "right" => Ok(Stick::Right),
        "left" => Ok(Stick::Left),
        other => Err(format!(
            "invalid stick '{}' (expected 'right' or 'left')",
            other
        )),
    }
}

fn parse_curve(s: &str) -> Result<Curve, String> {
    match s.to_ascii_lowercase().as_str() {
        "linear" => Ok(Curve::Linear),
        "ease-in" | "easein" => Ok(Curve::EaseIn),
        other => Err(format!(
            "invalid curve '{}' (expected 'linear' or 'ease-in')",
            other
        )),
    }
}

fn parse_button(s: &str) -> Result<Option<X360Button>, String> {
    if s.eq_ignore_ascii_case("none") || s.is_empty() {
        return Ok(None);
    }
    X360Button::parse(s).map(Some).ok_or_else(|| {
        format!(
            "invalid button '{}' (use A, B, X, Y, LB, RB, LS, RS, START, BACK, \
             DPAD_UP, DPAD_DOWN, DPAD_LEFT, DPAD_RIGHT, GUIDE, or 'none')",
            s
        )
    })
}

fn validate_range(name: &str, value: f64, lo: f64, hi: f64) -> Result<(), String> {
    if value.is_finite() && value >= lo && value <= hi {
        Ok(())
    } else {
        Err(format!(
            "{} must be in [{}, {}] (got {})",
            name, lo, hi, value
        ))
    }
}

impl Config {
    /// Build a Config from defaults, an optional TOML file, and CLI overrides.
    ///
    /// `cli` values (Some(...)) override both the file and the defaults.
    #[allow(clippy::too_many_arguments)]
    pub fn from_parts(
        path: Option<&Path>,
        cli_stick: Option<&str>,
        cli_sensitivity: Option<f64>,
        cli_decay: Option<f64>,
        cli_deadzone: Option<f64>,
        cli_invert_x: Option<bool>,
        cli_invert_y: Option<bool>,
        cli_curve: Option<&str>,
    ) -> Result<Config, String> {
        let file = match path {
            Some(p) => {
                let raw = std::fs::read_to_string(p).map_err(|e| {
                    format!(
                        "failed to read config '{}': {}",
                        p.display(),
                        e
                    )
                })?;
                toml::from_str::<FileConfig>(&raw).map_err(|e| {
                    format!("invalid TOML in '{}': {}", p.display(), e)
                })?
            }
            None => FileConfig::default(),
        };

        let mut cfg = MappingConfig::default();

        let stick_s = cli_stick.map(String::from).or(file.stick);
        if let Some(s) = stick_s {
            cfg.stick = parse_stick(&s)?;
        }

        let sensitivity = cli_sensitivity.or(file.sensitivity);
        if let Some(v) = sensitivity {
            validate_range("sensitivity", v, 0.0, 100.0)?;
            cfg.sensitivity = v;
        }

        let decay = cli_decay.or(file.decay);
        if let Some(v) = decay {
            validate_range("decay", v, 0.0, 1.0)?;
            cfg.decay = v;
        }

        let deadzone = cli_deadzone.or(file.deadzone);
        if let Some(v) = deadzone {
            validate_range("deadzone", v, 0.0, 1000.0)?;
            cfg.deadzone = v;
        }

        cfg.invert_x = cli_invert_x.or(file.invert_x).unwrap_or(false);
        cfg.invert_y = cli_invert_y.or(file.invert_y).unwrap_or(false);

        let curve_s = cli_curve.map(String::from).or(file.curve);
        if let Some(s) = curve_s {
            cfg.curve = parse_curve(&s)?;
        }

        let mouse_left_button = match file.mouse_left_button {
            Some(s) => parse_button(&s)?,
            None => None,
        };
        let mouse_right_button = match file.mouse_right_button {
            Some(s) => parse_button(&s)?,
            None => None,
        };
        let wheel_up = match file.wheel_up {
            Some(s) => parse_button(&s)?,
            None => None,
        };
        let wheel_down = match file.wheel_down {
            Some(s) => parse_button(&s)?,
            None => None,
        };

        Ok(Config {
            mapping: cfg,
            mouse_left_button,
            mouse_right_button,
            wheel_up,
            wheel_down,
        })
    }
}