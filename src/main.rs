//! stickmap — bridge physical mouse movement to a virtual Xbox 360
//! right-stick (or left-stick) via ViGEm.

mod config;
mod mapping;
mod mouse;
mod vgamepad;

use clap::Parser;
use config::Config;
use log::{debug, error, info};
use mapping::{AxisState, MappingConfig, Stick};
use mouse::{Accumulators, MouseSource, RdevMouseSource};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use vgamepad::{PClient, PConnectedTarget, SendTarget, VigemClient, XUSBReport};

/// Tick rate of the axis loop (125 Hz => every 8 ms).
const TICK_MS: u64 = 8;

#[derive(Parser, Debug)]
#[command(
    name = "stickmap",
    version,
    about = "Bridge mouse movement to a virtual Xbox 360 stick via ViGEm."
)]
struct Cli {
    /// Path to a TOML config file.
    #[arg(long, value_name = "FILE")]
    config: Option<std::path::PathBuf>,

    /// Axis units of deflection per pixel of mouse travel.
    #[arg(long)]
    sensitivity: Option<f64>,

    /// Stick spring-back multiplier per tick (1.0 = hold mode).
    #[arg(long)]
    decay: Option<f64>,

    /// Ignore deltas below this many pixels before scaling.
    #[arg(long)]
    deadzone: Option<f64>,

    /// Which stick to drive: right (default) or left.
    #[arg(long, value_parser = ["right", "left"])]
    stick: Option<String>,

    /// Invert the X axis.
    #[arg(long)]
    invert_x: bool,

    /// Invert the Y axis.
    #[arg(long)]
    invert_y: bool,

    /// Initialise the pad, sweep the mapped axis, log values, then exit.
    #[arg(long)]
    selftest: bool,
}

fn main() {
    env_logger::init();

    let cli = Cli::parse();

    let cfg = match Config::from_parts(
        cli.config.as_deref(),
        cli.stick.as_deref(),
        cli.sensitivity,
        cli.decay,
        cli.deadzone,
        cli.invert_x.then_some(true),
        cli.invert_y.then_some(true),
        None, // curve: CLI not exposed in the required flag set
    ) {
        Ok(c) => c,
        Err(e) => {
            error!("invalid configuration: {e}");
            std::process::exit(2);
        }
    };

    info!(
        "stickmap starting — stick={:?} sensitivity={} decay={} deadzone={} invert_x={} invert_y={} curve={:?}",
        cfg.mapping.stick,
        cfg.mapping.sensitivity,
        cfg.mapping.decay,
        cfg.mapping.deadzone,
        cfg.mapping.invert_x,
        cfg.mapping.invert_y,
        cfg.mapping.curve
    );

    // Load ViGEm client DLL.
    let vigem = match VigemClient::load() {
        Ok(v) => v,
        Err(e) => {
            error!("{e}");
            error!(
                "ViGEm client library (vigemclient.dll) could not be loaded.\n\
                 Please install the ViGEmBus driver from\n\
                 https://github.com/ViGEm/ViGEmBus/releases\n\
                 and try again."
            );
            std::process::exit(1);
        }
    };

    if cli.selftest {
        selftest(&vigem, &cfg.mapping);
        return;
    }

    run(&vigem, &cfg);
}
/// Build an XUSB report from the current axis state and button bitmask.
fn make_report(state: &AxisState, mapping: &MappingConfig, buttons: u16) -> XUSBReport {
    let mut r = XUSBReport {
        w_buttons: buttons,
        ..Default::default()
    };
    let vx = state.x.value as i16;
    let vy = state.y.value as i16;
    match mapping.stick {
        Stick::Right => {
            r.s_thumb_rx = vx;
            r.s_thumb_ry = vy;
        }
        Stick::Left => {
            r.s_thumb_lx = vx;
            r.s_thumb_ly = vy;
        }
    }
    r
}

/// Main loop: run the mouse listener + 125 Hz tick thread until Ctrl+C.
fn run(vigem: &VigemClient, cfg: &Config) {
    let client: PClient = vigem.alloc_client();
    if client.is_null() {
        error!("vigem_alloc failed (out of memory?)");
        return;
    }
    if let Err(e) = vigem.connect(client) {
        error!("vigem_connect failed: {e}");
        error!(
            "Please install the ViGEmBus driver from\n\
             https://github.com/ViGEm/ViGEmBus/releases"
        );
        return;
    }

    let target: PConnectedTarget = vigem.alloc_target();
    if target.is_null() {
        error!("vigem_target_x360_alloc failed");
        vigem.free_client(client);
        return;
    }
    if let Err(e) = vigem.connect_target(client, target) {
        error!("vigem_connect_target failed: {e}");
        vigem.free_client(client);
        return;
    }
    info!("virtual Xbox 360 controller connected to the bus");

    let acc = Arc::new(Accumulators::new());
    let buttons = Arc::new(std::sync::atomic::AtomicU16::new(0));
    let left_mask = cfg.mouse_left_button.map(|b| b.mask()).unwrap_or(0);
    let right_mask = cfg.mouse_right_button.map(|b| b.mask()).unwrap_or(0);
    let wheel_up_mask = cfg.wheel_up.map(|b| b.mask()).unwrap_or(0);
    let wheel_down_mask = cfg.wheel_down.map(|b| b.mask()).unwrap_or(0);
    let mouse_src = RdevMouseSource::new(
        Arc::clone(&acc),
        Arc::clone(&buttons),
        left_mask,
        right_mask,
        wheel_up_mask,
        wheel_down_mask,
    );
    if let Err(e) = mouse_src.start() {
        error!("failed to start mouse listener: {e}");
        return;
    }
    info!("mouse listener started (pass-through, no cursor capture)");

    let running = Arc::new(AtomicBool::new(true));
    let r = Arc::clone(&running);
    ctrlc::set_handler(move || {
        info!("received Ctrl+C, shutting down...");
        r.store(false, Ordering::SeqCst);
    })
    .map_err(|e| {
        error!("failed to install Ctrl+C handler: {e}");
    })
    .ok();

    // Tick thread + main sleep loop managed within a thread scope so non-'static
    // references (to `vigem`, `cfg`, `acc`, `buttons`, `running`) are borrowed safely.
    let target_t = SendTarget(target);
    std::thread::scope(|s| {
        s.spawn(|| {
            let mut state = AxisState::default();
            loop {
                if !running.load(Ordering::SeqCst) {
                    break;
                }
                let (dx, dy) = acc.drain();
                let (x, y) = state.tick(dx, dy, &cfg.mapping);
                let btn_val = buttons.load(Ordering::Relaxed);
                if log::log_enabled!(log::Level::Debug) {
                    debug!("tick dx={dx} dy={dy} -> ({x},{y}) buttons=0x{btn_val:04X}");
                }
                let report = make_report(&state, &cfg.mapping, btn_val);
                if let Err(e) = vigem.set_state(target_t.ptr(), &report) {
                    error!("set_state failed: {e}");
                }
                std::thread::sleep(Duration::from_millis(TICK_MS));
            }
        });

        while running.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(50));
        }
    });

    vigem.disconnect_target(client, target);
    vigem.free_client(client);
    info!("shutdown complete");
}

/// Self-test: initialise the pad, sweep the mapped axis through a known
/// pattern, log every value, then disconnect and exit.
fn selftest(vigem: &VigemClient, mapping: &MappingConfig) {
    info!("selftest: initialising virtual Xbox 360 controller");
    let client: PClient = vigem.alloc_client();
    if client.is_null() {
        error!("vigem_alloc failed");
        return;
    }
    if let Err(e) = vigem.connect(client) {
        error!("vigem_connect failed: {e}");
        error!(
            "Please install the ViGEmBus driver from\n\
             https://github.com/ViGEm/ViGEmBus/releases"
        );
        return;
    }
    let target: PConnectedTarget = vigem.alloc_target();
    if target.is_null() {
        error!("vigem_target_x360_alloc failed");
        vigem.free_client(client);
        return;
    }
    if let Err(e) = vigem.connect_target(client, target) {
        error!("vigem_connect_target failed: {e}");
        vigem.free_client(client);
        return;
    }
    info!(
        "pad attached — check Game Controllers (joy.cpl) or Steam's\n\
           controller test page for the new device"
    );
    info!("sweeping mapped axis through a known pattern...");

    let mut state = AxisState::default();
    let sweep = [
        0, 1000, 3000, 8000, 16000, 24000, 32767, 32767, 20000, 12000, 5000, 1500, 0, 0, -2000,
        -9000, -18000, -28000, -32767, -32767, -15000, -4000, 0,
    ];
    for (i, delta) in sweep.iter().enumerate() {
        let (x, y) = state.tick(*delta, *delta, mapping);
        info!("selftest[{i:02}] delta={delta:>5} -> x={x:>6} y={y:>6}");
        let report = make_report(&state, mapping, 0);
        if let Err(e) = vigem.set_state(target, &report) {
            error!("set_state failed: {e}");
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    info!("selftest complete — stick should have moved and returned to center");
    vigem.disconnect_target(client, target);
    vigem.free_client(client);
}
