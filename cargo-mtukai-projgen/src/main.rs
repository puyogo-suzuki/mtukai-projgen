use anyhow::Result;
use clap::{Parser, Subcommand};
use std::fs;
use std::path::{Path, PathBuf};

mod cargo_toml;
mod project_clone;

const GEN_DIR: &str = "generated";

/// Configuration for CLI operations
#[derive(Debug, Clone)]
struct Config {
    manifest: PathBuf,
    destination_path: PathBuf,
    verbose: bool,
}

impl Config {
    fn new(manifest_path: Option<PathBuf>, output_dir: Option<PathBuf>, verbose: bool) -> Self {
        let manifest = 
            manifest_path
                .and_then(|p| {
                    if p.is_file() {
                        p.parent().map(|parent| parent.to_path_buf())
                    } else {
                        Some(p.to_path_buf())
                    }
                })
                .unwrap_or_else(|| PathBuf::from("./"));
        let destination_path = manifest.join(output_dir.clone().unwrap_or_else(|| PathBuf::from(GEN_DIR)));
        Config {
            manifest,
            destination_path,
            verbose,
        }
    }
}

/// Verbose println! macro
macro_rules! vprintln {
    ($config:expr, $($arg:tt)*) => {
        if $config.verbose {
            eprintln!($($arg)*);
        }
    };
}

/// Simple CLI for cargo-mtukai-projgen
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Generate project structure
    Gen {
        #[arg(long = "manifest-path", value_name = "PATH")]
        manifest_path: Option<PathBuf>,
        #[arg(long = "output-dir", value_name = "DIR")]
        output_dir: Option<PathBuf>,
        #[arg(long = "cargo-toml")]
        cargo_toml: bool,
        #[arg(short, long)]
        verbose: bool,
    },
    /// Build generated projects
    Build {
        #[arg(long = "manifest-path", value_name = "PATH")]
        manifest_path: Option<PathBuf>,
        #[arg(long = "output-dir", value_name = "DIR")]
        output_dir: Option<PathBuf>,
        #[arg(short, long)]
        verbose: bool,
    },
    /// Run generated projects
    Run {
        #[arg(long = "manifest-path", value_name = "PATH")]
        manifest_path: Option<PathBuf>,
        #[arg(long = "output-dir", value_name = "DIR")]
        output_dir: Option<PathBuf>,
        #[arg(short, long)]
        verbose: bool,
    },
}

fn main() -> Result<()> {
    let args = Args::parse();
    match args.command {
        Commands::Gen {
            manifest_path,
            output_dir,
            cargo_toml,
            verbose,
        } => {
            cmd_gen(Config::new(manifest_path, output_dir, verbose), cargo_toml)
        }
        Commands::Build {
            manifest_path,
            output_dir,
            verbose,
        } => {
            cmd_build(Config::new(manifest_path, output_dir, verbose))
        }
        Commands::Run {
            manifest_path,
            output_dir,
            verbose,
        } => {
            cmd_run(Config::new(manifest_path, output_dir, verbose))
        }
    }
}

fn cmd_gen(config: Config, cargo_toml: bool) -> Result<()> {
    let source = config.manifest;
    let destination = config.destination_path;
    vprintln!(config, "Source: {}", source.display());
    vprintln!(config, "Destination: {}", destination.display());
    let cargo_toml_data = cargo_toml::CargoToml::new(source.join("Cargo.toml"))?;
    if cargo_toml {
        println!("Main Cargo.toml:\n{}", cargo_toml_data.generate_main_file()?);
        println!("LP Cargo.toml:\n{}", cargo_toml_data.generate_lp_file()?);
        return Ok(());
    }
    gen_main_project(&source, &destination, &cargo_toml_data)?;
    gen_lp_project(&source, &destination, &cargo_toml_data)?;
    println!("Full project clone completed.");
    Ok(())
}

fn cmd_build(config: Config) -> Result<()> {
    cmd_gen(config.clone(), false)?;
    let destination = config.destination_path;
    vprintln!(config, "Building projects in: {}", destination.display());
    let main_path = destination.join("main");
    let lp_path = destination.join("lp");
    if lp_path.exists() {
        println!("Building LP project...");
        todo!("Building the LP project is not implemented yet.");
        vprintln!(config, "LP project path: {}", lp_path.display());
    }
    if main_path.exists() {
        println!("Building main project...");
        todo!("Building the main project is not implemented yet.");
        vprintln!(config, "Main project path: {}", main_path.display());
    }
    println!("Build completed.");
    Ok(())
}

fn cmd_run(config: Config) -> Result<()> {
    cmd_build(config.clone())?;
    let destination = config.destination_path;
    vprintln!(config, "Running projects in: {}", destination.display());
    let main_path = destination.join("main");
    if main_path.exists() {
        println!("Running main project...");
        todo!("Running the main project is not implemented yet.");
        vprintln!(config, "Main project path: {}", main_path.display());
    }
    println!("Run completed.");
    Ok(())
}

fn is_ignored_main(path: &Path, src: &Path, _dst: &Path) -> bool {
    let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
    if path.starts_with(src.join(".cargo")) {
        return false;
    }
    name.starts_with(".")
}

fn gen_main_project(
    source: &Path,
    destination_origin: &Path,
    cargo_toml: &cargo_toml::CargoToml,
) -> Result<()> {
    let destination = &destination_origin.join("main");
    project_clone::clone_project(&source, &destination_origin, &destination, is_ignored_main)?;
    let main_cargo_toml = cargo_toml.generate_main_file()?.to_string();
    fs::write(&destination.join("Cargo.toml"), main_cargo_toml)?;
    Ok(())
}

fn is_ignored_lp(path: &Path, src: &Path, _dst: &Path) -> bool {
    let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
    name.starts_with(".") || path.starts_with(src.join("build.rs"))
}

fn gen_lp_project(
    source: &Path,
    destination_origin: &Path,
    cargo_toml: &cargo_toml::CargoToml,
) -> Result<()> {
    let destination = &destination_origin.join("lp");
    project_clone::clone_project(&source, &destination_origin, &destination, is_ignored_lp)?;
    let lp_cargo_toml = cargo_toml.generate_lp_file()?.to_string();
    fs::write(&destination.join("Cargo.toml"), lp_cargo_toml)?;
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
