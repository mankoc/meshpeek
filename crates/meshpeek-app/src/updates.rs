//! Help → Check for updates: asks GitHub for the latest MeshPeek release and compares it with this build.
//! Nothing is checked automatically; the request happens on a background thread only when the user asks.

use std::sync::{Arc, Mutex};

use eframe::egui;
use serde::Deserialize;

/// This build's version.
const CURRENT: &str = env!("CARGO_PKG_VERSION");
/// The project page (`https://github.com/<owner>/<repo>`), from the workspace's `repository` field.
const REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");

/// What the check found.
#[derive(Debug, Clone, PartialEq)]
pub enum Status {
    /// The request is running.
    Checking,
    /// The latest release is not newer than this build.
    UpToDate { latest: String },
    /// A newer release exists.
    Available { version: String, url: String, notes: String },
    /// GitHub could not be reached or answered something unexpected.
    Failed(String),
}

/// The update window and the result of the last check.
#[derive(Default)]
pub struct Updates {
    open: bool,
    status: Arc<Mutex<Option<Status>>>,
}

/// The parts of GitHub's release description that are used.
#[derive(Deserialize)]
struct Release {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    body: String,
}

impl Updates {
    /// Open the window and start a check (unless one is already running).
    pub fn check(&mut self, ctx: &egui::Context) {
        self.open = true;
        let mut status = self.status.lock().unwrap();
        if *status == Some(Status::Checking) {
            return;
        }
        *status = Some(Status::Checking);
        let (shared, ctx) = (self.status.clone(), ctx.clone());
        std::thread::spawn(move || {
            let result = latest_release().map_or_else(Status::Failed, |release| compare(release, CURRENT));
            *shared.lock().unwrap() = Some(result);
            ctx.request_repaint();
        });
    }

    /// The update window, while it is open.
    pub fn window(&mut self, ctx: &egui::Context) {
        let status = self.status.lock().unwrap().clone();
        egui::Window::new("Check for updates")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .open(&mut self.open)
            .show(ctx, |ui| {
                ui.set_width(380.0);
                match status {
                    None | Some(Status::Checking) => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label("Looking for a newer version…");
                        });
                    }
                    Some(Status::UpToDate { latest }) => {
                        ui.label(format!("MeshPeek {CURRENT} is up to date."));
                        ui.weak(format!("Latest release: {latest}"));
                    }
                    Some(Status::Available { version, url, notes }) => {
                        ui.heading(format!("MeshPeek {version} is available"));
                        ui.label(format!("You have version {CURRENT}."));
                        ui.add_space(4.0);
                        if !notes.trim().is_empty() {
                            ui.label("What's new:");
                            egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                                ui.label(notes.trim());
                            });
                            ui.add_space(4.0);
                        }
                        if ui.button("Open the download page").clicked() {
                            ui.ctx().open_url(egui::OpenUrl::new_tab(url));
                        }
                    }
                    Some(Status::Failed(reason)) => {
                        ui.colored_label(ui.visuals().warn_fg_color, "Could not check for updates.");
                        ui.weak(reason);
                        if ui.button("Open the releases page").clicked() {
                            ui.ctx().open_url(egui::OpenUrl::new_tab(format!("{REPOSITORY}/releases")));
                        }
                    }
                }
            });
    }
}

/// Ask GitHub for the latest published release, through the system's `curl` (built into Windows 10 and 11 and
/// installed on practically every Linux desktop), so MeshPeek needs no HTTP or TLS code of its own.
fn latest_release() -> Result<Release, String> {
    let path = REPOSITORY.trim_end_matches('/').strip_prefix("https://github.com/").ok_or("unknown repository")?;
    let url = format!("https://api.github.com/repos/{path}/releases/latest");
    let mut curl = std::process::Command::new("curl");
    curl.args(["--silent", "--show-error", "--location", "--max-time", "15"])
        .args(["--header", "Accept: application/vnd.github+json"])
        .args(["--user-agent", &format!("MeshPeek/{CURRENT}")])
        .args(["--write-out", "\n%{http_code}", &url]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000; // no console window flashing up
        curl.creation_flags(CREATE_NO_WINDOW);
    }
    let output = curl.output().map_err(|_| "the curl program was not found on this computer".to_string())?;
    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr);
        return Err(format!("GitHub could not be reached ({})", error.trim().trim_start_matches("curl: ")));
    }
    // the body, then the HTTP status code on the last line (--write-out)
    let text = String::from_utf8_lossy(&output.stdout);
    let (body, code) = text.rsplit_once('\n').unwrap_or((&text, ""));
    match code.trim() {
        "200" => serde_json::from_str(body).map_err(|e| format!("unexpected answer from GitHub ({e})")),
        "404" => Err("no releases have been published yet".into()),
        other => Err(format!("GitHub answered with status {other}")),
    }
}

/// Compare a release with this build's `current` version.
fn compare(release: Release, current: &str) -> Status {
    let latest = release.tag_name.trim_start_matches(['v', 'V']).to_string();
    match (version(&latest), version(current)) {
        (Some(new), Some(have)) if new > have => {
            Status::Available { version: latest, url: release.html_url, notes: release.body }
        }
        (Some(_), Some(_)) => Status::UpToDate { latest },
        _ => Status::Failed(format!("unrecognised release name \"{}\"", release.tag_name)),
    }
}

/// `major.minor.patch` as numbers (a pre-release suffix such as `-beta` is ignored).
fn version(text: &str) -> Option<(u64, u64, u64)> {
    let core = text.split(['-', '+']).next()?;
    let mut parts = core.split('.').map(|p| p.parse::<u64>().ok());
    let major = parts.next()??;
    Some((major, parts.next().flatten().unwrap_or(0), parts.next().flatten().unwrap_or(0)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(tag: &str) -> Release {
        Release { tag_name: tag.into(), html_url: "https://example/r".into(), body: "notes".into() }
    }

    #[test]
    fn newer_releases_are_offered() {
        assert!(matches!(compare(release("v0.5.0"), "0.4.1"), Status::Available { version, .. } if version == "0.5.0"));
        assert!(matches!(compare(release("v0.4.10"), "0.4.9"), Status::Available { .. }));
    }

    #[test]
    fn same_or_older_releases_are_up_to_date() {
        assert_eq!(compare(release("v0.4.1"), "0.4.1"), Status::UpToDate { latest: "0.4.1".into() });
        assert!(matches!(compare(release("0.3.0"), "0.4.1"), Status::UpToDate { .. }));
    }

    #[test]
    fn odd_tags_are_reported() {
        assert!(matches!(compare(release("nightly"), "0.4.1"), Status::Failed(_)));
        assert_eq!(version("1.2.3-beta"), Some((1, 2, 3)));
        assert_eq!(version("2"), Some((2, 0, 0)));
    }
}
