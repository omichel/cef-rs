use std::env;
use std::fs;
use std::path::Path;

#[cfg(windows)]
fn compile_windows_resources(manifest_dir: &str) {
    let rc_path = Path::new(manifest_dir).join("src/win/shell.rc");
    if rc_path.exists() {
        let _ = embed_resource::compile(&rc_path, embed_resource::NONE);
    }
}

fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    if !dst.exists() {
        fs::create_dir_all(dst)?;
    }
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

fn main() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let out_dir = env::var("OUT_DIR").unwrap();

    // OUT_DIR is something like target/debug/build/shell-xxx/out
    // We need to go up to target/debug or target/release
    let out_path = Path::new(&out_dir);
    let target_dir = out_path
        .ancestors()
        .nth(3)
        .expect("Failed to find target directory");

    let src_htdocs = Path::new(&manifest_dir).join("htdocs");
    let dst_htdocs = target_dir.join("htdocs");

    if src_htdocs.exists() {
        copy_dir_all(&src_htdocs, &dst_htdocs).expect("Failed to copy htdocs directory");
    }

    // Copy the icons directory
    let src_icons = Path::new(&manifest_dir).join("icons");
    let dst_icons = target_dir.join("icons");
    if src_icons.exists() {
        copy_dir_all(&src_icons, &dst_icons).expect("Failed to copy icons directory");
    }

    // Copy the Windows manifest file
    let src_manifest = Path::new(&manifest_dir).join("src/win/shell.exe.manifest");
    let dst_manifest = target_dir.join("shell.exe.manifest");
    if src_manifest.exists() {
        fs::copy(&src_manifest, &dst_manifest).expect("Failed to copy manifest file");
    }

    // Tell Cargo to rerun this script if htdocs, icons, or manifest changes
    println!("cargo:rerun-if-changed=htdocs");
    println!("cargo:rerun-if-changed=icons");
    println!("cargo:rerun-if-changed=src/win/shell.exe.manifest");
    println!("cargo:rerun-if-changed=src/win/shell.rc");
    println!("cargo:rerun-if-changed=src/win/icon.ico");

    // Compile Windows resources (executable icon for File Explorer)
    #[cfg(windows)]
    compile_windows_resources(&manifest_dir);
}
