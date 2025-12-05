//! Package cefdemo for Windows distribution
//! Usage: cargo dist

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const APP_NAME: &str = "Cresus";
const EXE_NAME: &str = "cefdemo.exe";

fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<u64> {
    let mut total_size = 0u64;
    if !dst.exists() {
        fs::create_dir_all(dst)?;
    }
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if ty.is_dir() {
            total_size += copy_dir_all(&src_path, &dst_path)?;
        } else {
            total_size += fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(total_size)
}

fn copy_file(src: &Path, dst: &Path) -> std::io::Result<u64> {
    if src.exists() {
        fs::copy(src, dst)
    } else {
        Ok(0)
    }
}

fn get_cef_path() -> PathBuf {
    if let Ok(cef_path) = env::var("CEF_PATH") {
        PathBuf::from(cef_path)
    } else {
        let home = env::var("USERPROFILE")
            .or_else(|_| env::var("HOME"))
            .expect("Could not find home directory");
        PathBuf::from(home).join(".local/share/cef")
    }
}

fn main() {
    // Always build release for distribution
    let build_config = "release";

    // Find repository root (go up from examples/cefdemo/src)
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let manifest_path = Path::new(manifest_dir);
    let repo_root = manifest_path
        .parent()
        .and_then(|p| p.parent())
        .expect("Failed to find repo root");

    let cef_path = get_cef_path();
    if !cef_path.exists() {
        eprintln!("Error: CEF binaries not found at {}", cef_path.display());
        eprintln!(
            "Please run: cargo run -p export-cef-dir -- --force $USERPROFILE/.local/share/cef"
        );
        std::process::exit(1);
    }

    // Build the application
    println!(
        "Building cefdemo in {} mode...",
        build_config.to_uppercase()
    );

    let mut cmd = Command::new("cargo");
    cmd.arg("build")
        .arg("--bin")
        .arg("cefdemo")
        .arg("--release");
    cmd.current_dir(repo_root);

    let status = cmd.status().expect("Failed to run cargo build");
    if !status.success() {
        eprintln!("Build failed!");
        std::process::exit(1);
    }

    let target_dir = repo_root.join("target").join(build_config);
    let dist_dir = manifest_path.join("dist").join(APP_NAME);

    // Create distribution directory
    println!("Creating distribution directory: {}", dist_dir.display());
    if dist_dir.exists() {
        fs::remove_dir_all(&dist_dir).expect("Failed to remove existing dist directory");
    }
    fs::create_dir_all(&dist_dir).expect("Failed to create dist directory");

    let mut total_size = 0u64;
    let mut file_count = 0u32;

    // Copy application files
    println!("Copying application files...");
    total_size += copy_file(&target_dir.join(EXE_NAME), &dist_dir.join(EXE_NAME))
        .expect("Failed to copy executable");
    file_count += 1;

    if let Ok(size) = copy_file(
        &target_dir.join("cefdemo.exe.manifest"),
        &dist_dir.join("cefdemo.exe.manifest"),
    ) {
        if size > 0 {
            total_size += size;
            file_count += 1;
        }
    }

    // Copy htdocs
    let htdocs_src = target_dir.join("htdocs");
    if htdocs_src.exists() {
        println!("Copying htdocs...");
        let size =
            copy_dir_all(&htdocs_src, &dist_dir.join("htdocs")).expect("Failed to copy htdocs");
        total_size += size;
        file_count += fs::read_dir(&dist_dir.join("htdocs"))
            .map(|r| r.count() as u32)
            .unwrap_or(0);
    }

    // Copy icons
    let icons_src = target_dir.join("icons");
    if icons_src.exists() {
        println!("Copying icons...");
        let size = copy_dir_all(&icons_src, &dist_dir.join("icons")).expect("Failed to copy icons");
        total_size += size;
        file_count += fs::read_dir(&dist_dir.join("icons"))
            .map(|r| r.count() as u32)
            .unwrap_or(0);
    }

    // Copy CEF runtime files
    println!("Copying CEF runtime files...");

    let cef_dlls = [
        "libcef.dll",
        "chrome_elf.dll",
        "d3dcompiler_47.dll",
        "dxcompiler.dll",
        "dxil.dll",
        "libEGL.dll",
        "libGLESv2.dll",
        "vk_swiftshader.dll",
        "vulkan-1.dll",
    ];

    for dll in &cef_dlls {
        if let Ok(size) = copy_file(&cef_path.join(dll), &dist_dir.join(dll)) {
            if size > 0 {
                total_size += size;
                file_count += 1;
            }
        }
    }

    let cef_resources = [
        "chrome_100_percent.pak",
        "chrome_200_percent.pak",
        "icudtl.dat",
        "resources.pak",
        "v8_context_snapshot.bin",
        "vk_swiftshader_icd.json",
    ];

    for res in &cef_resources {
        if let Ok(size) = copy_file(&cef_path.join(res), &dist_dir.join(res)) {
            if size > 0 {
                total_size += size;
                file_count += 1;
            }
        }
    }

    // Copy locales directory (only en-US to save space)
    let locales_src: PathBuf = cef_path.join("locales");
    let locales_dst = dist_dir.join("locales");
    if locales_src.exists() {
        println!("Copying locales...");
        fs::create_dir_all(&locales_dst).expect("Failed to create locales directory");
        
        // Only copy en-US locale
        let locale_file = "en-US.pak";
        if let Ok(size) = copy_file(&locales_src.join(locale_file), &locales_dst.join(locale_file)) {
            if size > 0 {
                total_size += size;
                file_count += 1;
            }
        }
    }

    let total_mb = total_size as f64 / (1024.0 * 1024.0);

    println!();
    println!("========================================");
    println!("Distribution package created successfully!");
    println!("========================================");
    println!("Location: {}", dist_dir.display());
    println!("Files: {}", file_count);
    println!("Total size: {:.2} MB", total_mb);
    println!();
    println!("To run the application:");
    println!("  cd \"{}\"", dist_dir.display());
    println!("  .\\{}", EXE_NAME);
}
