//! Runtime FFI to ViGEmClient (vigemclient.dll) using libloading.
//!
//! We load the DLL at runtime so there is NO link-time dependency - the
//! driver just needs to be installed on the host.

use libloading::Library;

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

pub const VIGEM_ERROR_NONE: i32 = 0x2000_0000;

fn ok(code: i32) -> Result<(), String> {
    if code == VIGEM_ERROR_NONE {
        Ok(())
    } else {
        Err(format!("ViGEm error 0x{:08X}", code))
    }
}

pub type PClient = *mut std::ffi::c_void;
pub type PConnectedTarget = *mut std::ffi::c_void;
/// Safe wrapper around a raw ViGEm target handle so it can cross threads.
///
/// ViGEm targets are opaque driver objects; the pointer is safe to share
/// because all access goes through the single owning client connection.
pub struct SendTarget(pub PConnectedTarget);
unsafe impl Send for SendTarget {}
unsafe impl Sync for SendTarget {}
impl SendTarget {
    /// The raw ViGEm target handle, for use across threads.
    ///
    /// Exposed through this method (rather than the public tuple field) so that
    /// a background closure captures `&SendTarget` (which is `Sync`) instead of
    /// borrowing the inner `*mut c_void` field directly.
    pub fn ptr(&self) -> PConnectedTarget {
        self.0
    }
}
/// Loaded runtime wrapper around vigemclient.dll.
pub struct VigemClient {
    _lib: Library,
    alloc_client: unsafe extern "C" fn() -> PClient,
    connect: unsafe extern "C" fn(PClient) -> i32,
    alloc_x360: unsafe extern "C" fn() -> PConnectedTarget,
    connect_target: unsafe extern "C" fn(PClient, PConnectedTarget) -> i32,
    x360_update: unsafe extern "C" fn(PConnectedTarget, *const XUSBReport) -> i32,
    disconnect_target: unsafe extern "C" fn(PClient, PConnectedTarget) -> i32,
    free: unsafe extern "C" fn(PClient),
}
impl VigemClient {
    /// Load vigemclient.dll and resolve all symbols.
    pub fn load() -> Result<Self, String> {
        unsafe {
            let lib = Library::new("vigemclient.dll")
                .map_err(|e| format!("failed to load vigemclient.dll: {e}"))?;

            // Dereference each `Symbol` to a plain fn pointer immediately so
            // the temporary borrow of `lib` ends before it is moved into Self.
            let alloc_client: unsafe extern "C" fn() -> PClient = *lib
                .get(b"vigem_alloc\0")
                .map_err(|e| format!("symbol vigem_alloc missing: {e}"))?;
            let connect: unsafe extern "C" fn(PClient) -> i32 = *lib
                .get(b"vigem_connect\0")
                .map_err(|e| format!("symbol vigem_connect missing: {e}"))?;
            let alloc_x360: unsafe extern "C" fn() -> PConnectedTarget = *lib
                .get(b"vigem_target_x360_alloc\0")
                .map_err(|e| format!("symbol vigem_target_x360_alloc missing: {e}"))?;
            let connect_target: unsafe extern "C" fn(PClient, PConnectedTarget) -> i32 = *lib
                .get(b"vigem_connect_target\0")
                .map_err(|e| format!("symbol vigem_connect_target missing: {e}"))?;
            let x360_update: unsafe extern "C" fn(PConnectedTarget, *const XUSBReport) -> i32 =
                *lib.get(b"vigem_target_x360_update\0")
                    .map_err(|e| format!("symbol vigem_target_x360_update missing: {e}"))?;
            let disconnect_target: unsafe extern "C" fn(PClient, PConnectedTarget) -> i32 = *lib
                .get(b"vigem_disconnect_target\0")
                .map_err(|e| format!("symbol vigem_disconnect_target missing: {e}"))?;
            let free: unsafe extern "C" fn(PClient) = *lib
                .get(b"vigem_free\0")
                .map_err(|e| format!("symbol vigem_free missing: {e}"))?;

            Ok(Self {
                _lib: lib,
                alloc_client,
                connect,
                alloc_x360,
                connect_target,
                x360_update,
                disconnect_target,
                free,
            })
        }
    }

    /// Create a new client handle.
    pub fn alloc_client(&self) -> PClient {
        unsafe { (self.alloc_client)() }
    }

    /// Connect to the ViGEmBus driver. Errors out when the driver is absent.
    pub fn connect(&self, client: PClient) -> Result<(), String> {
        unsafe { ok((self.connect)(client)) }
    }

    /// Create a new Xbox 360 target handle.
    pub fn alloc_target(&self) -> PConnectedTarget {
        unsafe { (self.alloc_x360)() }
    }

    /// Plug the target into the bus (device becomes visible to Windows).
    pub fn connect_target(&self, client: PClient, target: PConnectedTarget) -> Result<(), String> {
        unsafe { ok((self.connect_target)(client, target)) }
    }

    /// Push a full XUSB report to the virtual pad.
    pub fn set_state(&self, target: PConnectedTarget, report: &XUSBReport) -> Result<(), String> {
        unsafe { ok((self.x360_update)(target, report)) }
    }

    /// Unplug and release the target.
    pub fn disconnect_target(&self, client: PClient, target: PConnectedTarget) {
        unsafe {
            (self.disconnect_target)(client, target);
        }
    }

    /// Free the client handle (must be the last ViGEm call).
    pub fn free_client(&self, client: PClient) {
        unsafe { (self.free)(client) }
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
