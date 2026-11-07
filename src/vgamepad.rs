//! Virtual Xbox 360 gamepad via the pure-Rust `vigem-client` crate.
//!
//! The `vigem-client` crate implements the ViGEmBus client in 100% Rust,
//! talking to the driver via WinAPI `DeviceIoControl` directly — no
//! `vigemclient.dll` and no other external DLL is needed.

/// XInput-compatible gamepad report (12 bytes, same layout as
/// `XINPUT_GAMEPAD` / the ViGEm XUSB report).
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct XUSBReport {
    pub w_buttons: u16,
    pub b_left_trigger: u8,
    pub b_right_trigger: u8,
    pub s_thumb_lx: i16,
    pub s_thumb_ly: i16,
    pub s_thumb_rx: i16,
    pub s_thumb_ry: i16,
}

/// Virtual Xbox 360 pad backed by the ViGEmBus driver.
#[cfg(windows)]
pub struct VigemPad {
    target: vigem_client::Xbox360Wired<vigem_client::Client>,
}

#[cfg(windows)]
impl VigemPad {
    /// Connect to the ViGEmBus driver, plug in a virtual Xbox 360 pad, and
    /// wait until it is ready. The pad is unplugged automatically on drop.
    pub fn connect() -> Result<Self, String> {
        let client = vigem_client::Client::connect()
            .map_err(|e| format!("failed to connect to ViGEmBus driver: {e}"))?;
        let mut target =
            vigem_client::Xbox360Wired::new(client, vigem_client::TargetId::XBOX360_WIRED);
        target
            .plugin()
            .map_err(|e| format!("failed to plug in virtual Xbox 360 controller: {e}"))?;
        target
            .wait_ready()
            .map_err(|e| format!("virtual Xbox 360 controller not ready: {e}"))?;
        Ok(Self { target })
    }

    /// Push a full XUSB report to the virtual pad.
    pub fn set_state(&mut self, report: &XUSBReport) -> Result<(), String> {
        let gamepad = vigem_client::XGamepad {
            buttons: vigem_client::XButtons::from(report.w_buttons),
            left_trigger: report.b_left_trigger,
            right_trigger: report.b_right_trigger,
            thumb_lx: report.s_thumb_lx,
            thumb_ly: report.s_thumb_ly,
            thumb_rx: report.s_thumb_rx,
            thumb_ry: report.s_thumb_ry,
        };
        self.target
            .update(&gamepad)
            .map_err(|e| format!("failed to update virtual Xbox 360 controller: {e}"))
    }
}

/// Stub for non-Windows hosts so the crate still compiles / tests
/// (e.g. `cargo test` on Linux) without the Windows-only `vigem-client`.
#[cfg(not(windows))]
pub struct VigemPad {
    _private: (),
}

#[cfg(not(windows))]
impl VigemPad {
    /// Connect to the ViGEmBus driver (Windows only).
    pub fn connect() -> Result<Self, String> {
        Err(
            "ViGEmBus driver is only available on Windows (install it from \
             https://github.com/ViGEm/ViGEmBus/releases)"
                .to_string(),
        )
    }

    /// Push a full XUSB report to the virtual pad (Windows only).
    pub fn set_state(&mut self, _report: &XUSBReport) -> Result<(), String> {
        Err("ViGEmBus driver is only available on Windows".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::{offset_of, size_of};

    #[test]
    fn xusb_report_layout_matches_spec() {
        assert_eq!(size_of::<XUSBReport>(), 12, "XUSBReport must be 12 bytes");
        assert_eq!(offset_of!(XUSBReport, w_buttons), 0);
        assert_eq!(offset_of!(XUSBReport, b_left_trigger), 2);
        assert_eq!(offset_of!(XUSBReport, b_right_trigger), 3);
        assert_eq!(offset_of!(XUSBReport, s_thumb_lx), 4);
        assert_eq!(offset_of!(XUSBReport, s_thumb_ly), 6);
        assert_eq!(offset_of!(XUSBReport, s_thumb_rx), 8);
        assert_eq!(offset_of!(XUSBReport, s_thumb_ry), 10);
    }
}
