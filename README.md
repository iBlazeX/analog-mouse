# analog-mouse

Bridge physical mouse movement to a virtual Xbox 360 controller stick, so
games that only accept gamepad sticks for camera movement respond to the
mouse as if a real stick were being moved. The system cursor keeps working
normally — no capture, no event swallowing.

Built on the **ViGEmBus** driver via the pure-Rust
[`vigem-client`](https://github.com/CasualX/vigem-client) crate
(no `vigemclient.dll` or any other external DLL — the client talks to
the driver directly through WinAPI).

## Prerequisites

Install the **ViGEmBus** driver on Windows (required at runtime):

1. Download the latest release from
   [ViGEm/ViGEmBus](https://github.com/ViGEm/ViGEmBus/releases).
2. Run the installer (reboot if asked).

## Build (cross-compile to Windows)

The toolchain is **llvm-mingw** (clang + LLD). The linker and link flags are
configured in `.cargo/config.toml`; the `libs/` directory holds two empty
stub archives that satisfy the `libgcc`/`libgcc_eh` flags Rust's windows-gnu
target passes at link time.

```bash
rustup target add x86_64-pc-windows-gnu
cargo build --release --target x86_64-pc-windows-gnu
```

The self-contained executable (no external DLL required beyond the
Windows system DLLs, and no external exception-handling DLL) is at
`target/x86_64-pc-windows-gnu/release/stickmap.exe`.

## Run

```bash
stickmap.exe                 # defaults: right stick
stickmap.exe --sensitivity 0.03 --decay 0.9 --stick left --invert-x
stickmap.exe --config myconfig.toml
stickmap.exe --selftest      # sweep the stick through a known pattern, then exit
RUST_LOG=debug stickmap.exe  # per-tick logging
```

Full configuration options (sensitivity, decay, deadzone, button mapping,
curve, etc.) are documented in `config.example.toml`.
