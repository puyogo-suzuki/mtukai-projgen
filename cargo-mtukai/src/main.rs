use anyhow::Result;
use clap::{Parser, Subcommand};
use std::{fs, path::{Path, PathBuf}, process::Command};

mod cargo_toml;
mod project_clone;

const GEN_DIR: &str = "generated";

/// Configuration for CLI operations
#[derive(Debug, Clone)]
struct Config {
    manifest: PathBuf,
    destination_path: PathBuf,
    build_name : String,
    release: bool,
    verbose: bool
}

impl Config {
    fn new(manifest_path: Option<PathBuf>, output_dir: Option<PathBuf>, build_name: Option<String>, release: bool, verbose: bool) -> Self {
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
            build_name: build_name.unwrap_or_else(|| "default".to_string()),
            release,
            verbose
        }
    }
    fn get_destination_lp(&self) -> PathBuf {
        self.destination_path.join("lp")
    }
    fn get_destination_main(&self) -> PathBuf {
        self.destination_path.join("main")
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
    #[arg(long = "manifest-path", short = 'm', value_name = "PATH")]
    manifest_path: Option<PathBuf>,
    #[arg(long = "output-dir", short = 'o', value_name = "DIR")]
    output_dir: Option<PathBuf>,
    #[arg(long = "build-name", short = 'b', value_name = "BUILD_NAME")]
    build_name: Option<String>,
    #[arg(short, long)]
    release: bool,
    #[arg(short, long)]
    verbose: bool,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Generate project structure
    Gen {
        #[arg(long = "cargo-toml")]
        cargo_toml: bool,
    },
    /// Build generated projects
    Build {},
    /// Run generated projects
    Run {},
}

fn main() -> Result<()> {
    let args = Args::parse();
    let conf = Config::new(args.manifest_path, args.output_dir, args.build_name, args.release, args.verbose);
    match args.command {
        Commands::Gen {
            cargo_toml
        } => cmd_gen(conf, cargo_toml),
        Commands::Build {} => cmd_build(conf),
        Commands::Run {} => cmd_run(conf)
    }.map(|_| ())
}

fn cmd_gen(config: Config, cargo_toml: bool) -> Result<cargo_toml::CargoToml> {
    let source = config.manifest;
    let destination = config.destination_path;
    vprintln!(config, "Source: {}", source.display());
    vprintln!(config, "Destination: {}", destination.display());
    let cargo_toml_data = cargo_toml::CargoToml::new(source.join("Cargo.toml"))?;
    if cargo_toml {
        println!("Main Cargo.toml:\n{}", cargo_toml_data.generate_main_file()?);
        println!("LP Cargo.toml:\n{}", cargo_toml_data.generate_lp_file()?);
        return Ok(cargo_toml_data);
    }
    gen_main_project(&source, &destination, &cargo_toml_data)?;
    gen_lp_project(&source, &destination, &cargo_toml_data)?;
    println!("Full project clone completed.");
    Ok(cargo_toml_data)
}

fn build_lp(config: &Config, build_config : &cargo_toml::BuildConfig, envs : &std::collections::HashMap<String, String>) -> Result<()> {
    let lp_path = config.get_destination_lp();
    vprintln!(config, "LP project path: {}", lp_path.display());
    if !lp_path.exists() {
        return Err(anyhow::anyhow!("LP project not found at {}", lp_path.display()));
    }
    let mut args = vec!["build".to_owned()];
    if let Some(lp_target) = &build_config.lp_target {
        args.push(format!("--target={}", lp_target));
    }
    if let Some(lp_features) = &build_config.lp_features {
        args.push(format!("--features={}", lp_features));
    }
    if let Some(lp_args) = &build_config.lp_args {
        args.extend(shell_words::split(lp_args).map_err(|e| anyhow::anyhow!("Failed to parse lp_args: {}", e))?);
    }
    let status = Command::new("cargo")
        .args(&args)
        .env_clear()
        .envs(envs)
        .current_dir(&lp_path)
        .status()?;
    if !status.success() {
        Err(anyhow::anyhow!("cargo build failed for LP project"))
    } else {
        Ok(())
    }
}

fn impl_cargo_exec_main(config: &Config, _build_config : &cargo_toml::BuildConfig, envs : &std::collections::HashMap<String, String>, cmd : &str) -> Result<()> {
    let main_path = config.get_destination_main();
    vprintln!(config, "Main project path: {}", main_path.display());
    if !main_path.exists() {
        return Err(anyhow::anyhow!("Main project not found at {}", main_path.display()));
    }
    let args = if config.release { &[cmd, "--release"] as &[&str] } else { &[cmd] as &[&str] };
    let status = Command::new("cargo")
        .args(args)
        .env_clear()
        .envs(envs)
        .current_dir(&main_path)
        .status()?;
    if !status.success() {
        Err(anyhow::anyhow!("cargo {} failed for main project", cmd))
    } else {
        Ok(())
    }
}

fn build_main(config: &Config, build_config : &cargo_toml::BuildConfig, envs : &std::collections::HashMap<String, String>) -> Result<()> {
    impl_cargo_exec_main(config, build_config, envs, "build")
}

fn run_main(config: &Config, build_config : &cargo_toml::BuildConfig, envs : &std::collections::HashMap<String, String>) -> Result<()> {
    impl_cargo_exec_main(config, build_config, envs, "run")
}

fn get_filtered_env() -> std::collections::HashMap<String, String> {
    std::env::vars().filter(|&(ref k, _)|
        !(k.starts_with("CARGO_") || k.starts_with("RUSTUP_") || k.starts_with("RUST_"))
    ).collect()
}

fn cmd_build(config: Config) -> Result<cargo_toml::CargoToml> {
    let cargo_toml = cmd_gen(config.clone(), false)?;
    let filtered_env = get_filtered_env();
    vprintln!(config, "Building projects in: {}", config.destination_path.display());
    let build_config = cargo_toml.get_build_config(config.build_name.clone()).ok_or_else(|| anyhow::anyhow!("Build configuration not found"))?;
    println!("Building LP project...");
    build_lp(&config, build_config, &filtered_env)?;
    println!("Building main project...");
    build_main(&config, build_config, &filtered_env)?;
    println!("Build completed.");
    Ok(cargo_toml)
}

fn cmd_run(config: Config) -> Result<cargo_toml::CargoToml> {
    let cargo_toml = cmd_gen(config.clone(), false)?;
    let filtered_env = get_filtered_env();
    vprintln!(config, "Running projects in: {}", config.destination_path.display());
    let build_config = cargo_toml.get_build_config(config.build_name.clone()).ok_or_else(|| anyhow::anyhow!("Build configuration not found"))?;
    println!("Building LP project...");
    build_lp(&config, build_config, &filtered_env)?;
    println!("Running main project...");
    run_main(&config, build_config, &filtered_env)?;
    println!("Run completed.");
    Ok(cargo_toml)
}

fn dont_copy_main(path: &Path, src: &Path, _dst: &Path) -> bool {
    let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
    if path.starts_with(src.join(".cargo")) {
        return false;
    }
    name.starts_with(".")
}
fn dont_delete_main(path: &Path, _src: &Path, dst: &Path) -> bool {
    let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
    if path.starts_with(dst.join(".cargo")) {
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
    project_clone::clone_project(&source, &destination_origin, &destination, dont_delete_main, dont_copy_main)?;
    let main_cargo_toml = cargo_toml.generate_main_file()?.to_string();
    fs::write(&destination.join("Cargo.toml"), main_cargo_toml)?;
    Ok(())
}

fn dont_copy_lp(path: &Path, src: &Path, _dst: &Path) -> bool {
    let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
    name.starts_with(".") || path.starts_with(src.join("build.rs"))
}
fn dont_delete_lp(path: &Path, _src: &Path, dst: &Path) -> bool {
    let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
    name.starts_with(".") || path.starts_with(dst.join("build.rs")) || path.starts_with(dst.join("ld"))
}

fn gen_lp_project(
    source: &Path,
    destination_origin: &Path,
    cargo_toml: &cargo_toml::CargoToml,
) -> Result<()> {
    let destination = &destination_origin.join("lp");
    project_clone::clone_project(&source, &destination_origin, &destination, dont_delete_lp, dont_copy_lp)?;
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
