use anyhow::{Context, Result};
use cargo_metadata::MetadataCommand;
use std::fs::{remove_dir_all, create_dir_all};
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

mod cargo_toml;

const GEN_DIR : &str = "generated";

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

fn clone_project(src: &Path, dst: &Path,  is_ignored : fn(&Path, &Path, &Path, &[PathBuf],) -> bool) -> Result<()> {
    // if dst.exists() {
    //     fs::remove_dir_all(dst)?;
    // }

    let blacklist = [src.join(GEN_DIR), src.join("Cargo.toml"), src.join("Cargo.lock")];

    for entry in WalkDir::new(src)
        .into_iter()
        .filter_entry(|e| !(is_ignored(e.path(), src, dst, &blacklist) || e.path() == dst))
    {
        let entry = entry?;
        let path = entry.path();

        let relative = path.strip_prefix(src)?;
        let target_path = dst.join(relative);

        if entry.file_type().is_dir() {
            fs::create_dir_all(&target_path)?;
        } else {
            if let Some(parent) = target_path.parent() {
                fs::create_dir_all(parent)?;
            }

            // Compare modification times - only copy if source is newer than target
            let should_copy = if target_path.exists() {
                let target_metadata = fs::metadata(&target_path)?;
                let source_metadata = fs::metadata(path)?;
                
                let target_modified = target_metadata.modified()?;
                let source_modified = source_metadata.modified()?;
                
                let res = source_modified > target_modified;
                if res {
                    let mut perm = target_metadata.permissions();
                    perm.set_readonly(false);
                    fs::set_permissions(&target_path, perm)?;
                }
                res
            } else {
                true
            };

            if should_copy {
                fs::copy(path, &target_path)
                    .with_context(|| format!("Failed to copy {:?}", path))?;

                let mut perms = fs::metadata(&target_path)?.permissions();
                perms.set_readonly(true);
                fs::set_permissions(&target_path, perms)?;
            }
        }
    }

    Ok(())
}

fn is_ignored_main(path: &Path, src: &Path, dst: &Path, blacklist: &[PathBuf]) -> bool {
    let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");

    if path.starts_with(src.join(".cargo")) {
        return false;
    }
    if name == "target"
        || name.starts_with(".")
        || blacklist.iter().any(|p| path.starts_with(p)) {
        return true;
    }
    false
}

fn gen_main_project(source: &Path, destination: &Path, cargo_toml: &cargo_toml::CargoToml) -> Result<()> {
    let destination = &destination.join("main");
    // Clone the project
    clone_project(&source, &destination, is_ignored_main)?;

    // Generate Cargo.toml
    let main_cargo_toml = cargo_toml.generate_main_file()?.to_string();
    fs::write(&destination.join("Cargo.toml"), main_cargo_toml)?;
    Ok(())
}

fn is_ignored_lp(path: &Path, src: &Path, dst: &Path, blacklist: &[PathBuf]) -> bool {
    let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");

    if name == "target"
        || name.starts_with(".")
        || blacklist.iter().any(|p| path.starts_with(p))
        || path.starts_with(src.join("build.rs")) {
        return true;
    }
    false
}

fn gen_lp_project(source: &Path, destination: &Path, cargo_toml: &cargo_toml::CargoToml) -> Result<()> {
    let destination = &destination.join("lp");
    // Clone the project
    clone_project(&source, &destination, is_ignored_lp)?;

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