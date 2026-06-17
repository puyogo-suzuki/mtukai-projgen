use anyhow::{Context, Result};
use cargo_metadata::MetadataCommand;
use std::fs;
use std::path::{Path, PathBuf};

mod cargo_toml;
mod project_clone;

fn get_manifest_path() -> Option<String> {
    let mut args = std::env::args().skip_while(|val| !val.starts_with("--manifest-path"));
    match args.next() {
        Some(ref p) if p == "--manifest-path" => args.next(),
        Some(p) => Some(p.trim_start_matches("--manifest-path").to_string()),
        None => None,
    }
}

// fn __cargo_metadata_test() -> Result<()> {
//     let mut cmd = MetadataCommand::new();
//     if let Some(manifest_path) = get_manifest_path() {
//         cmd.manifest_path(manifest_path);
//     }
    
//     let metadata = cmd.exec().context("Failed to get cargo metadata")?;
//     let root_package = metadata.root_package().context("Failed to find root package")?;
//     println!("Root package: {}", root_package.name);

//     let target_dir = metadata.target_directory.clone().into_std_path_buf();
//     let offload_dir = target_dir.join("copro");
//     if offload_dir.exists() {
//         remove_dir_all(&offload_dir).context("Failed to remove offload directory")?;
//     }
//     create_dir_all(&offload_dir).context("Failed to create offload directory")?;
//     println!("The offload directory is located at: {}", offload_dir.display());
// }

const GEN_DIR : &str = "generated";

fn main() -> Result<()> {
    let manifest_path = get_manifest_path();

    let source = manifest_path.map(|v| {
        let path = PathBuf::from(v);
        if let Some(parent) = path.parent() {
            parent.to_path_buf()
        } else {
            path
        }
    }).unwrap_or(PathBuf::from("./")); // Current Project
    let destination = source.join(GEN_DIR);

    let cargo_toml = cargo_toml::CargoToml::new(source.join("Cargo.toml"))?;

    if std::env::args().any(|arg| arg == "--cargo-toml") {
        println!("Main Cargo.toml:\n{}", cargo_toml.generate_main_file()?);
        println!("LP Cargo.toml:\n{}", cargo_toml.generate_lp_file()?);
        return Ok(());
    }

    gen_main_project(&source, &destination, &cargo_toml)?;
    gen_lp_project(&source, &destination, &cargo_toml)?;

    println!("Full project clone completed.");
    Ok(())
}

fn is_ignored_main(path: &Path, src: &Path, dst: &Path) -> bool {
    let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");

    if path.starts_with(src.join(".cargo")) {
        return false;
    }
    name.starts_with(".")
}

fn gen_main_project(source: &Path, destination_origin: &Path, cargo_toml: &cargo_toml::CargoToml) -> Result<()> {
    let destination = &destination_origin.join("main");
    // Clone the project
    project_clone::clone_project(&source, &destination_origin, &destination, is_ignored_main)?;

    // Generate Cargo.toml
    let main_cargo_toml = cargo_toml.generate_main_file()?.to_string();
    fs::write(&destination.join("Cargo.toml"), main_cargo_toml)?;
    Ok(())
}

fn is_ignored_lp(path: &Path, src: &Path, _dst: &Path) -> bool {
    let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");

    name.starts_with(".") || path.starts_with(src.join("build.rs"))
}

fn gen_lp_project(source: &Path, destination_origin: &Path, cargo_toml: &cargo_toml::CargoToml) -> Result<()> {
    let destination = &destination_origin.join("lp");
    // Clone the project
    project_clone::clone_project(&source, &destination_origin, &destination, is_ignored_lp)?;

    // Generate Cargo.toml
    let lp_cargo_toml = cargo_toml.generate_lp_file()?.to_string();
    fs::write(&destination.join("Cargo.toml"), lp_cargo_toml)?;

    // Generate build.toml and the linker script.
    let build_rs = include_str!("lp_build_rs.txt");
    fs::write(&destination.join("build.rs"), build_rs)?;
    if let Ok(false) = fs::exists(&destination.join("ld")) {
        fs::create_dir(&destination.join("ld"))?;
    }
    let link_lp_x = include_str!("lp_ld_link_lp_x.txt");
    fs::write(&destination.join("ld").join("link-lp.x"), link_lp_x)?;
    let link_ulp_x = include_str!("lp_ld_link_ulp_x.txt");
    fs::write(&destination.join("ld").join("link-ulp.x"), link_ulp_x)?;
    Ok(())
}