#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;

use eframe::egui;

/// Intel-драйвер Vulkan на этой машине (igvk64.dll 30.0.101.1003) падает с
/// access violation при инициализации wgpu, поэтому Vulkan отключён явно.
fn wgpu_options() -> eframe::WgpuConfiguration {
    let mut config = eframe::WgpuConfiguration::default();
    if let eframe::egui_wgpu::WgpuSetup::CreateNew(setup) = &mut config.wgpu_setup {
        setup.instance_descriptor.backends = eframe::wgpu::Backends::DX12;
    }
    config
}

fn main() -> anyhow::Result<()> {
    let conn = opora::db::open_default()?;

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([720.0, 560.0])
            .with_min_inner_size([520.0, 420.0])
            .with_title("OPORA"),
        wgpu_options: wgpu_options(),
        ..Default::default()
    };

    eframe::run_native(
        "OPORA",
        options,
        Box::new(move |_cc| Ok(Box::new(app::App::new(conn)))),
    )?;
    Ok(())
}
