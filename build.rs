use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=assets/app.ico");
    generate_window_icon().expect("failed to generate window icon data");

    #[cfg(windows)]
    {
        let mut resources = winres::WindowsResource::new();
        resources.set_icon("assets/app.ico");
        resources
            .compile()
            .expect("failed to compile Windows resources");
    }
}

fn generate_window_icon() -> Result<(), Box<dyn std::error::Error>> {
    let icon_bytes = fs::read("assets/app.ico")?;
    let icon_image =
        image::load_from_memory_with_format(&icon_bytes, image::ImageFormat::Ico)?.into_rgba8();

    let width = icon_image.width();
    let height = icon_image.height();
    let rgba = icon_image.into_raw();
    let out_dir = PathBuf::from(env::var("OUT_DIR")?);

    fs::write(out_dir.join("app_icon.rgba"), rgba)?;
    fs::write(
        out_dir.join("app_icon.rs"),
        format!(
            "fn load_window_icon() -> eframe::egui::IconData {{\n    eframe::egui::IconData {{\n        rgba: include_bytes!(concat!(env!(\"OUT_DIR\"), \"/app_icon.rgba\")).to_vec(),\n        width: {width},\n        height: {height},\n    }}\n}}\n"
        ),
    )?;

    Ok(())
}
