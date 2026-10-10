//! MeshPeek: fast viewer for STL, OBJ, PLY and Lychee `.lys` files.
//!
//!     meshpeek [--cpu] [files...]
//!
//! `--cpu` forces the software renderer (WARP on Windows, lavapipe/llvmpipe on Linux).

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // no console window in release builds

mod app;
mod help;
mod loading;
mod settings;
mod spacemouse;
mod viewport;

use std::path::PathBuf;
use std::sync::Arc;

use eframe::egui_wgpu::{WgpuConfiguration, WgpuSetup, WgpuSetupCreateNew};
use meshpeek_render::wgpu;

fn main() -> eframe::Result {
    env_logger::init();
    let mut cpu = false;
    let mut files = Vec::new();
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--cpu" => cpu = true,
            _ => files.push(PathBuf::from(arg)),
        }
    }

    let setup = WgpuSetupCreateNew {
        // Ask for everything the adapter offers: large buffers for big models, and line drawing for wireframe.
        device_descriptor: Arc::new(|adapter| wgpu::DeviceDescriptor {
            label: Some("meshpeek"),
            required_features: adapter.features() & wgpu::Features::POLYGON_MODE_LINE,
            required_limits: adapter.limits(),
            ..Default::default()
        }),
        native_adapter_selector: cpu.then(|| {
            Arc::new(|adapters: &[wgpu::Adapter], _surface: Option<&wgpu::Surface<'_>>| {
                adapters
                    .iter()
                    .find(|a| a.get_info().device_type == wgpu::DeviceType::Cpu)
                    .cloned()
                    .ok_or_else(|| "no software renderer found (on Linux, install mesa-vulkan-drivers)".to_string())
            }) as eframe::egui_wgpu::NativeAdapterSelectorMethod
        }),
        ..Default::default()
    };
    let mut viewport = eframe::egui::ViewportBuilder::default()
        .with_title("MeshPeek")
        .with_inner_size([1400.0, 900.0])
        .with_drag_and_drop(true);
    if let Ok(icon) = image::load_from_memory(include_bytes!("../assets/icon.png")) {
        let icon = icon.to_rgba8();
        let (width, height) = icon.dimensions();
        viewport = viewport.with_icon(eframe::egui::IconData { rgba: icon.into_raw(), width, height });
    }
    let options = eframe::NativeOptions {
        viewport,
        renderer: eframe::Renderer::Wgpu,
        wgpu_options: WgpuConfiguration { wgpu_setup: WgpuSetup::CreateNew(setup), ..Default::default() },
        ..Default::default()
    };
    eframe::run_native("MeshPeek", options, Box::new(|cc| Ok(Box::new(app::MeshPeekApp::new(cc, files)))))
}
