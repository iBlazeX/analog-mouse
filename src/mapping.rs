//! Pure, platform-independent mapping math.
//!
//! This module has NO Windows or IO dependencies so that the full mapping
//! behaviour can be exercised by ordinary `cargo test` on any host.

/// Maximum absolute value of an X360 stick axis (both thumbs use ±32767).
pub const AXIS_MAX: i32 = 32767;

/// Which physical stick the mouse drives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stick {
    Right,
    Left,
}

/// Response-curve shapes supported by the mapper.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Curve {
    Linear,
    EaseIn,
}

/// All knobs that affect the mapping. Pure data — no I/O.
#[derive(Debug, Clone)]
pub struct MappingConfig {
    pub sensitivity: f64,
    pub decay: f64,
    pub deadzone: f64,
    pub invert_x: bool,
    pub invert_y: bool,
    pub stick: Stick,
    pub curve: Curve,
}

impl Default for MappingConfig {
    fn default() -> Self {
        Self {
            sensitivity: 0.02,
            decay: 0.85,
            deadzone: 0.0,
            invert_x: false,
            invert_y: false,
            stick: Stick::Right,
            curve: Curve::Linear,
        }
    }
}

/// Per-axis mutable state held across ticks.
#[derive(Debug, Clone, Copy)]
pub struct Axis {
    pub value: f64,
}

impl Default for Axis {
    fn default() -> Self {
        Self { value: 0.0 }
    }
}

impl Axis {
    /// One tick of the 125 Hz loop: apply decay, then the scaled delta,
    /// then the optional curve, then clamp to ±AXIS_MAX.
    pub fn tick(&mut self, delta: i32, cfg: &MappingConfig, invert: bool) -> i32 {
        let mut d = delta as f64;
        if cfg.deadzone > 0.0 && d.abs() < cfg.deadzone {
            d = 0.0;
        }
        if invert {
            d = -d;
        }

        let mut v = self.value * cfg.decay + d * cfg.sensitivity;
        v = apply_curve(v, cfg.curve);
        v = v.clamp(-(AXIS_MAX as f64), AXIS_MAX as f64).round();
        self.value = v;
        v as i32
    }
}

/// Apply the selected response curve to a raw axis value in [-32767, 32767].
pub fn apply_curve(value: f64, curve: Curve) -> f64 {
    match curve {
        Curve::Linear => value,
        Curve::EaseIn => {
            let n = (value / AXIS_MAX as f64).clamp(-1.0, 1.0);
            let sign = if n >= 0.0 { 1.0 } else { -1.0 };
            // Mild ease-in: x^1.5 preserves sign and centre behaviour.
            sign * n.abs().powf(1.5) * AXIS_MAX as f64
        }
    }
}

/// Convenience: run one tick for the X and Y axes given the drained deltas.
#[derive(Debug, Clone, Copy)]
pub struct AxisState {
    pub x: Axis,
    pub y: Axis,
}

impl Default for AxisState {
    fn default() -> Self {
        Self {
            x: Axis::default(),
            y: Axis::default(),
        }
    }
}

impl AxisState {
    pub fn tick(&mut self, dx: i32, dy: i32, cfg: &MappingConfig) -> (i32, i32) {
        let x = self.x.tick(dx, cfg, cfg.invert_x);
        let y = self.y.tick(dy, cfg, cfg.invert_y);
        (x, y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> MappingConfig {
        MappingConfig::default()
    }

    #[test]
    fn decay_converges_to_center() {
        // Push to max, then stop moving: with decay < 1 the axis must
        // monotonically return toward 0.
        let mut ax = Axis::default();
        let c = MappingConfig {
            sensitivity: 100.0, // saturates fast so we really hit the clamp
            decay: 0.85,
            ..cfg()
        };
        for _ in 0..50 {
            ax.tick(AXIS_MAX, &c, false);
        }
        assert_eq!(ax.value, AXIS_MAX as f64);

        for _ in 0..200 {
            ax.tick(0, &c, false);
        }
        assert!(
            ax.value.abs() < 1.0,
            "value should decay to ~0, got {}",
            ax.value
        );
    }

    #[test]
    fn sensitivity_scales_delta() {
        let mut ax = Axis::default();
        let c = MappingConfig { sensitivity: 2.0, decay: 0.0, ..cfg() };
        let out = ax.tick(10, &c, false);
        assert_eq!(out, 20);
    }

    #[test]
    fn deadzone_ignores_small_deltas() {
        let mut ax = Axis::default();
        let c = MappingConfig {
            deadzone: 5.0,
            sensitivity: 100.0,
            ..cfg()
        };
        ax.tick(3, &c, false); // below deadzone
        assert_eq!(ax.value, 0.0);
        ax.tick(8, &c, false); // above deadzone
        assert!(ax.value > 0.0);
    }

    #[test]
    fn clamps_at_32767() {
        let mut ax = Axis::default();
        let c = MappingConfig {
            sensitivity: 10.0,
            decay: 0.0,
            ..cfg()
        };
        let out = ax.tick(1_000_000, &c, false);
        assert_eq!(out, AXIS_MAX);
        assert_eq!(ax.value, AXIS_MAX as f64);
    }

    #[test]
    fn clamps_at_negative_32767() {
        let mut ax = Axis::default();
        let c = MappingConfig {
            sensitivity: 10.0,
            decay: 0.0,
            ..cfg()
        };
        let out = ax.tick(-1_000_000, &c, false);
        assert_eq!(out, -AXIS_MAX);
    }

    #[test]
    fn inversion_flips_sign() {
        let mut ax = Axis::default();
        let c = MappingConfig {
            sensitivity: 1.0,
            decay: 0.0,
            ..cfg()
        };
        let out = ax.tick(50, &c, true);
        assert_eq!(out, -50);
    }

    #[test]
    fn ease_in_curve_is_center_sensitive() {
        let lin = apply_curve(1000.0, Curve::Linear);
        let ease = apply_curve(1000.0, Curve::EaseIn);
        assert!(ease.abs() < lin.abs(), "ease-in dampens small inputs");
        assert!(ease > 0.0);
    }

    #[test]
    fn hold_mode_keeps_axis() {
        let mut ax = Axis::default();
        let c = MappingConfig {
            decay: 1.0, // hold
            sensitivity: 1.0,
            ..cfg()
        };
        ax.tick(100, &c, false);
        let v1 = ax.value;
        ax.tick(0, &c, false);
        assert_eq!(ax.value, v1, "decay=1 must hold the stick still");
    }
}