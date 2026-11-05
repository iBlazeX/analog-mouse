//! Mouse capture backend.
//!
//! Exposes a swappable `MouseSource` trait plus the shared `Accumulators`
//! that the mouse callback feeds and the 125 Hz tick loop drains. The
//! default backend uses `rdev`'s raw low-level hook, which is non-blocking
//! and always chains to `CallNextHookEx`, so the system mouse keeps working
//! normally (no capture, no cursor lock).

use std::sync::atomic::{AtomicI32, AtomicU16, Ordering};
use std::sync::{Arc, Mutex};

/// Accumulators shared between the mouse callback and the tick loop.
#[derive(Debug, Default)]
pub struct Accumulators {
    pub dx: AtomicI32,
    pub dy: AtomicI32,
}

impl Accumulators {
    pub fn new() -> Self {
        Self {
            dx: AtomicI32::new(0),
            dy: AtomicI32::new(0),
        }
    }

    /// Called inside the mouse callback to add a relative delta.
    pub fn add(&self, delta_x: i32, delta_y: i32) {
        self.dx.fetch_add(delta_x, Ordering::Relaxed);
        self.dy.fetch_add(delta_y, Ordering::Relaxed);
    }

    /// Drain accumulated deltas since the last tick (atomically resets).
    pub fn drain(&self) -> (i32, i32) {
        let x = self.dx.swap(0, Ordering::Relaxed);
        let y = self.dy.swap(0, Ordering::Relaxed);
        (x, y)
    }
}

/// Trait behind which swappable mouse capture backends live.
pub trait MouseSource: Send + Sync {
    /// Start listening for mouse events (spawns a background listener).
    fn start(&self) -> Result<(), String>;
}

/// rdev-based mouse backend.
///
/// rdev 0.5 delivers absolute coordinates on Windows via the low-level
/// hook, so we track the last position and turn it into a relative delta
/// before adding it to the accumulators. It also forwards configured
/// mouse buttons/wheel to the shared buttons bitmask.
pub struct RdevMouseSource {
    acc: Arc<Accumulators>,
    buttons: Arc<AtomicU16>,
    last: Arc<Mutex<Option<(f64, f64)>>>,
    left: u16,
    right: u16,
    wheel_up: u16,
    wheel_down: u16,
}

impl RdevMouseSource {
    pub fn new(
        acc: Arc<Accumulators>,
        buttons: Arc<AtomicU16>,
        left: u16,
        right: u16,
        wheel_up: u16,
        wheel_down: u16,
    ) -> Self {
        Self {
            acc,
            buttons,
            last: Arc::new(Mutex::new(None)),
            left,
            right,
            wheel_up,
            wheel_down,
        }
    }
}

impl MouseSource for RdevMouseSource {
    fn start(&self) -> Result<(), String> {
        let acc = Arc::clone(&self.acc);
        let buttons = Arc::clone(&self.buttons);
        let last = Arc::clone(&self.last);
        let left = self.left;
        let right = self.right;
        let wheel_up = self.wheel_up;
        let wheel_down = self.wheel_down;
        std::thread::Builder::new()
            .name("stickmap-mouse".into())
            .spawn(move || {
                let callback = move |event: rdev::Event| match event.event_type {
                    rdev::EventType::MouseMove { x, y } => {
                        let mut guard = last.lock().unwrap_or_else(|e| e.into_inner());
                        let delta = match *guard {
                            Some((px, py)) => {
                                let dx = (x - px).round() as i32;
                                let dy = (y - py).round() as i32;
                                (dx, dy)
                            }
                            None => (0, 0),
                        };
                        *guard = Some((x, y));
                        acc.add(delta.0, delta.1);
                    }
                    rdev::EventType::ButtonPress(rdev::Button::Left) if left != 0 => {
                        buttons.fetch_or(left, Ordering::Relaxed);
                    }
                    rdev::EventType::ButtonRelease(rdev::Button::Left) if left != 0 => {
                        buttons.fetch_and(!left, Ordering::Relaxed);
                    }
                    rdev::EventType::ButtonPress(rdev::Button::Right) if right != 0 => {
                        buttons.fetch_or(right, Ordering::Relaxed);
                    }
                    rdev::EventType::ButtonRelease(rdev::Button::Right) if right != 0 => {
                        buttons.fetch_and(!right, Ordering::Relaxed);
                    }
                    rdev::EventType::Wheel { delta_y, .. } if delta_y > 0 => {
                        if wheel_up != 0 {
                            buttons.fetch_or(wheel_up, Ordering::Relaxed);
                            std::thread::spawn({
                                let b = Arc::clone(&buttons);
                                move || {
                                    std::thread::sleep(std::time::Duration::from_millis(60));
                                    b.fetch_and(!wheel_up, Ordering::Relaxed);
                                }
                            });
                        }
                    }
                    rdev::EventType::Wheel { delta_y, .. } if delta_y < 0 => {
                        if wheel_down != 0 {
                            buttons.fetch_or(wheel_down, Ordering::Relaxed);
                            std::thread::spawn({
                                let b = Arc::clone(&buttons);
                                move || {
                                    std::thread::sleep(std::time::Duration::from_millis(60));
                                    b.fetch_and(!wheel_down, Ordering::Relaxed);
                                }
                            });
                        }
                    }
                    _ => {}
                };
                if let Err(err) = rdev::listen(callback) {
                    log::error!("rdev listen error: {:?}", err);
                }
            })
            .map_err(|e| format!("failed to spawn mouse listener thread: {e}"))?;

        Ok(())
    }
}

/// No-op mouse source used by platform-independent unit tests / non-Windows.
pub struct DummyMouseSource {
    pub acc: Arc<Accumulators>,
}

impl MouseSource for DummyMouseSource {
    fn start(&self) -> Result<(), String> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accumulators_add_and_drain() {
        let acc = Accumulators::new();
        acc.add(5, -3);
        acc.add(2, 4);
        assert_eq!(acc.drain(), (7, 1));
        assert_eq!(acc.drain(), (0, 0), "drain must reset to zero");
    }
}