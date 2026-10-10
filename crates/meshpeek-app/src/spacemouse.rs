//! 3Dconnexion space mouse input, without the vendor SDK.
//!
//! A background thread reads the device and keeps the latest 6-axis motion and button state in a shared
//! [`State`]; the viewport applies it every frame. Backends:
//!
//! - **spacenavd** (Linux): the open-source daemon's Unix socket, used first when the daemon runs (it then
//!   owns the device).
//! - **Raw HID** via `hidapi` (Windows without the 3DxWare driver, and Linux without spacenavd; on Linux the
//!   `.deb` installs a udev rule that grants access).
//!
//! Both are normalised to one convention, **x right, y up, z toward the user** for translation and rotation
//! about those axes, in device units (about ±350 at full deflection).

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use eframe::egui;

/// Latest motion and buttons, shared between the reader thread and the UI.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Motion {
    pub translation: [f32; 3],
    pub rotation: [f32; 3],
}

#[derive(Debug, Default)]
pub struct State {
    pub motion: Motion,
    /// When `motion` was last updated: a device that stops reporting is treated as centred.
    pub updated: Option<Instant>,
    /// Buttons pressed since the UI last looked (bit n = button n).
    pub pressed: u32,
    /// Human-readable name of the connected device/backend, if any.
    pub device: Option<String>,
}

pub type Shared = Arc<Mutex<State>>;

/// Motion older than this is ignored (some devices send nothing while the cap is at rest).
pub const STALE: Duration = Duration::from_millis(250);

/// 3Dconnexion USB vendor ids (old Logitech-era devices and current ones).
pub const VENDORS: [u16; 2] = [0x046d, 0x256f];

/// Start the reader thread. It keeps trying to (re)connect, so devices can be plugged in later.
pub fn start(ctx: egui::Context) -> Shared {
    let shared: Shared = Arc::default();
    let state = shared.clone();
    std::thread::Builder::new()
        .name("spacemouse".into())
        .spawn(move || {
            loop {
                #[cfg(unix)]
                if spacenavd::run(&state, &ctx) {
                    continue; // daemon connection ended; try again
                }
                if hid::run(&state, &ctx) {
                    continue;
                }
                std::thread::sleep(Duration::from_secs(2)); // nothing found yet
            }
        })
        .expect("spawn spacemouse thread");
    shared
}

/// Decoded content of one raw HID input report.
#[derive(Debug, PartialEq)]
pub enum Report {
    Translation([i16; 3]),
    Rotation([i16; 3]),
    /// Newer devices send all six axes in one report.
    Both([i16; 3], [i16; 3]),
    Buttons(u32),
}

/// Decode a raw HID report (first byte = report id), in HID axes; [`hid_to_motion`] converts them.
pub fn decode_hid(report: &[u8]) -> Option<Report> {
    let axes = |bytes: &[u8]| -> Option<[i16; 3]> {
        if bytes.len() < 6 {
            return None;
        }
        Some([0, 1, 2].map(|i| i16::from_le_bytes([bytes[i * 2], bytes[i * 2 + 1]])))
    };
    match report.first()? {
        1 if report.len() >= 13 => Some(Report::Both(axes(&report[1..7])?, axes(&report[7..13])?)),
        1 => Some(Report::Translation(axes(report.get(1..7)?)?)),
        2 => Some(Report::Rotation(axes(report.get(1..7)?)?)),
        3 => {
            let mut bits = 0u32;
            for (i, b) in report[1..].iter().take(4).enumerate() {
                bits |= (*b as u32) << (8 * i);
            }
            Some(Report::Buttons(bits))
        }
        _ => None,
    }
}

/// HID axes (x right, y away from the user, z down) -> ours (x right, y up, z toward the user).
pub fn hid_to_motion(v: [i16; 3]) -> [f32; 3] {
    [v[0] as f32, -(v[2] as f32), -(v[1] as f32)]
}

fn update(state: &Shared, ctx: &egui::Context, change: impl FnOnce(&mut State)) {
    let mut s = state.lock().unwrap();
    change(&mut s);
    s.updated = Some(Instant::now());
    drop(s);
    ctx.request_repaint();
}

mod hid {
    use super::*;

    /// Read from the first 3Dconnexion device found; returns false if there is none.
    pub fn run(state: &Shared, ctx: &egui::Context) -> bool {
        let Ok(api) = hidapi::HidApi::new() else { return false };
        let info = api.device_list().find(|d| {
            VENDORS.contains(&d.vendor_id()) && d.usage_page() == 0x01 && d.usage() == 0x08 // multi-axis controller
        });
        let Some(info) = info else { return false };
        let Ok(device) = info.open_device(&api) else { return false };
        let name = info.product_string().unwrap_or("3Dconnexion device").to_string();
        log::info!("space mouse: {name} (raw HID)");
        state.lock().unwrap().device = Some(format!("{name} (USB HID)"));
        let mut buffer = [0u8; 64];
        loop {
            match device.read_timeout(&mut buffer, 500) {
                Ok(0) => continue,
                Ok(n) => match decode_hid(&buffer[..n]) {
                    Some(Report::Translation(t)) => update(state, ctx, |s| s.motion.translation = hid_to_motion(t)),
                    Some(Report::Rotation(r)) => update(state, ctx, |s| s.motion.rotation = hid_to_motion(r)),
                    Some(Report::Both(t, r)) => update(state, ctx, |s| {
                        s.motion.translation = hid_to_motion(t);
                        s.motion.rotation = hid_to_motion(r);
                    }),
                    Some(Report::Buttons(bits)) => update(state, ctx, |s| s.pressed |= bits),
                    None => {}
                },
                Err(e) => {
                    log::warn!("space mouse disconnected: {e}");
                    state.lock().unwrap().device = None;
                    return true;
                }
            }
        }
    }
}

#[cfg(unix)]
mod spacenavd {
    use std::io::Read;
    use std::os::unix::net::UnixStream;

    use super::*;

    const SOCKET: &str = "/var/run/spnav.sock";

    /// Events from spacenavd: eight native-endian i32. Motion: [0, x, y, z, rx, ry, rz, period];
    /// button: [1 = press / 2 = release, number, ...]. spacenavd's axes: x right, y up, z away from the user.
    pub fn decode(event: [i32; 8]) -> Option<Report> {
        match event[0] {
            0 => Some(Report::Both(
                [event[1], event[2], event[3]].map(|v| v.clamp(-32768, 32767) as i16),
                [event[4], event[5], event[6]].map(|v| v.clamp(-32768, 32767) as i16),
            )),
            1 => Some(Report::Buttons(1u32.checked_shl(event[1] as u32).unwrap_or(0))),
            _ => None,
        }
    }

    pub fn run(state: &Shared, ctx: &egui::Context) -> bool {
        let Ok(mut stream) = UnixStream::connect(SOCKET) else { return false };
        log::info!("space mouse: spacenavd");
        state.lock().unwrap().device = Some("spacenavd".into());
        let flip_z = |v: [i16; 3]| [v[0] as f32, v[1] as f32, -(v[2] as f32)];
        let mut bytes = [0u8; 32];
        while stream.read_exact(&mut bytes).is_ok() {
            let event: [i32; 8] =
                std::array::from_fn(|i| i32::from_ne_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap()));
            match decode(event) {
                Some(Report::Both(t, r)) => update(state, ctx, |s| {
                    s.motion.translation = flip_z(t);
                    s.motion.rotation = flip_z(r);
                }),
                Some(Report::Buttons(bits)) => update(state, ctx, |s| s.pressed |= bits),
                _ => {}
            }
        }
        state.lock().unwrap().device = None;
        true
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn decodes_motion_and_buttons() {
            assert_eq!(decode([0, 10, -20, 30, 1, 2, 3, 16]), Some(Report::Both([10, -20, 30], [1, 2, 3])));
            assert_eq!(decode([1, 1, 0, 0, 0, 0, 0, 0]), Some(Report::Buttons(0b10)));
            assert_eq!(decode([2, 1, 0, 0, 0, 0, 0, 0]), None); // releases are ignored
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(id: u8, values: &[i16]) -> Vec<u8> {
        let mut bytes = vec![id];
        values.iter().for_each(|v| bytes.extend(v.to_le_bytes()));
        bytes
    }

    #[test]
    fn decodes_classic_split_reports() {
        assert_eq!(decode_hid(&report(1, &[100, -200, 300])), Some(Report::Translation([100, -200, 300])));
        assert_eq!(decode_hid(&report(2, &[-5, 6, -7])), Some(Report::Rotation([-5, 6, -7])));
    }

    #[test]
    fn decodes_combined_report_of_newer_devices() {
        let bytes = report(1, &[1, 2, 3, 4, 5, 6]);
        assert_eq!(decode_hid(&bytes), Some(Report::Both([1, 2, 3], [4, 5, 6])));
    }

    #[test]
    fn decodes_buttons_and_ignores_unknown_reports() {
        assert_eq!(decode_hid(&[3, 0b0000_0011, 0]), Some(Report::Buttons(0b11)));
        assert_eq!(decode_hid(&[7, 1, 2]), None);
        assert_eq!(decode_hid(&[1, 1]), None); // too short
    }

    #[test]
    fn hid_axes_become_right_up_toward_user() {
        // Pushing the cap away (HID +y) moves away from the user (our -z); pressing down (HID +z) is our -y.
        assert_eq!(hid_to_motion([10, 20, 30]), [10.0, -30.0, -20.0]);
    }
}
