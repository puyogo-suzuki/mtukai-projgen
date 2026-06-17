use anyhow::{Context, Result};
use cargo_metadata::MetadataCommand;
use clap::Parser;
use std::fs;
use std::path::{Path, PathBuf};

mod cargo_toml;
mod project_clone;

/// Simple CLI for cargo-mtukai-projgen
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to Cargo.toml (or directory containing it)
    #[arg(long = "manifest-path", value_name = "PATH")]
    manifest_path: Option<PathBuf>,

    /// Print generated Cargo.toml files to stdout instead of writing
    #[arg(long = "cargo-toml")]
    cargo_toml: bool,

    /// Output directory name for generated projects (default: generated)
    #[arg(long = "output-dir", value_name = "DIR")]
    output_dir: Option<PathBuf>,

    /// Verbose output
    #[arg(short, long)]
    verbose: bool,
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

const GEN_DIR: &str = "generated";

fn main() -> Result<()> {
    let args = Args::parse();

    let source = if let Some(manifest) = &args.manifest_path {
        // If a file was provided, use its parent; otherwise use the path as-is.
        if manifest.is_file() {
            manifest.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| manifest.to_path_buf())
        } else {
            manifest.to_path_buf()
        }
    } else {
        PathBuf::from("./")
    };

    let destination = source.join(args.output_dir.clone().unwrap_or_else(|| PathBuf::from(GEN_DIR)));

    if args.verbose {
        eprintln!("Source: {}", source.display());
        eprintln!("Destination: {}", destination.display());
    }

    let cargo_toml = cargo_toml::CargoToml::new(source.join("Cargo.toml"))?;

    if args.cargo_toml {
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