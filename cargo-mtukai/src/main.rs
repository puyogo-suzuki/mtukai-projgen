use anyhow::Result;
use clap::{Parser, Subcommand};
use std::{cell::RefCell, fs, path::{Path, PathBuf}, process::Command};
use crate::cargo_toml::BuildParameter;

mod cargo_toml;
mod project_clone;
mod chip_dic;

const GEN_DIR: &str = "generated";

/// Configuration for CLI operations
#[derive(Debug, Clone)]
struct Config {
    manifest: PathBuf,
    destination_path: PathBuf,
    template_name: RefCell<Option<String>>,
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
            template_name: RefCell::new(None),
            build_name: build_name.unwrap_or_else(|| "default".to_string()),
            release,
            verbose
        }
    }
    fn get_destination_path(&self) -> &PathBuf {
        &self.destination_path
    }
    fn get_destination_path_base(&self) -> PathBuf {
        match &*self.template_name.borrow() {
            Some(name) => self.get_destination_path().join(name),
            None => self.get_destination_path().clone()
        }
    }
    fn get_destination_lp_full(&self) -> PathBuf {
        self.get_destination_path().join(self.get_gen_double("lp"))
    }
    fn get_destination_main_full(&self) -> PathBuf {
        self.get_destination_path().join(self.get_gen_double("main"))
    }
    fn get_gen_double<S: AsRef<str>>(&self, proc_name: S) -> PathBuf {
        match &*self.template_name.borrow() {
            Some(name) => Path::new(name).join(proc_name.as_ref()),
            None => Path::new(proc_name.as_ref()).to_path_buf()
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
        Commands::Gen { cargo_toml } => cmd_gen(&conf, cargo_toml),
        Commands::Build {} => cmd_build(&conf),
        Commands::Run {} => cmd_run(&conf)
    }.map(|_| ())
}

fn cmd_gen(config: &Config, cargo_toml: bool) -> Result<cargo_toml::CargoToml> {
    let cargo_toml_data = cargo_toml::CargoToml::new(config.manifest.join("Cargo.toml"))?;
    let template_name = &cargo_toml_data.get_build_config(&config.build_name)
        .ok_or_else(|| anyhow::anyhow!("Build configuration not found"))?.template_name;
    config.template_name.replace(Some(template_name.clone()));
    vprintln!(config, "Source: {}", config.manifest.display());
    vprintln!(config, "Destination: {}", config.get_destination_path_base().display());
    if cargo_toml {
        println!("Main Cargo.toml:\n{}", cargo_toml_data.generate_main_file(&config.manifest)?);
        println!("LP Cargo.toml:\n{}", cargo_toml_data.generate_lp_file(&config.manifest)?);
        return Ok(cargo_toml_data);
    }
    gen_main_project(&config, &cargo_toml_data)?;
    gen_lp_project(&config, &cargo_toml_data)?;
    println!("Full project clone completed.");
    Ok(cargo_toml_data)
}

fn gen_args<S: AsRef<str>>(command : S, build_parameter : &BuildParameter, release : bool) -> Result<Vec<String>> {
    let mut args = vec![command.as_ref().to_owned()];
    if let Some(target) = &build_parameter.target {
        args.push(format!("--target={}", target));
    }
    if let Some(features) = &build_parameter.features {
        args.push(format!("--features={}", features));
    }
    if let Some(args_str) = &build_parameter.args {
        args.extend(shell_words::split(args_str).map_err(|e| anyhow::anyhow!("Failed to parse args: {}", e))?);
    }
    if release || build_parameter.release {
        args.push("--release".to_string());
    }
    Ok(args)
}

fn build_lp(config: &Config, build_config : &cargo_toml::BuildConfig, envs : &std::collections::HashMap<String, String>) -> Result<()> {
    let lp_path = config.get_destination_lp_full();
    vprintln!(config, "LP project path: {}", lp_path.display());
    if !lp_path.exists() {
        return Err(anyhow::anyhow!("LP project not found at {}", lp_path.display()));
    }
    let args = gen_args("build", &build_config.lp_params, config.release)?;
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

fn impl_cargo_exec_main(config: &Config, build_config : &cargo_toml::BuildConfig, envs : &std::collections::HashMap<String, String>, cmd : &str) -> Result<()> {
    let main_path = config.get_destination_main_full();
    vprintln!(config, "Main project path: {}", main_path.display());
    if !main_path.exists() {
        return Err(anyhow::anyhow!("Main project not found at {}", main_path.display()));
    }
    let args = gen_args(cmd, &build_config.main_params, config.release)?;
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

fn cmd_build(config: &Config) -> Result<cargo_toml::CargoToml> {
    let cargo_toml = cmd_gen(config, false)?;
    let filtered_env = get_filtered_env();
    vprintln!(config, "Building projects in: {}", config.destination_path.display());
    let build_config = cargo_toml.get_build_config(&config.build_name).ok_or_else(|| anyhow::anyhow!("Build configuration not found"))?;
    println!("Building LP project...");
    build_lp(config, build_config, &filtered_env)?;
    println!("Building main project...");
    build_main(config, build_config, &filtered_env)?;
    println!("Build completed.");
    Ok(cargo_toml)
}

fn cmd_run(config: &Config) -> Result<cargo_toml::CargoToml> {
    let cargo_toml = cmd_gen(config, false)?;
    let filtered_env = get_filtered_env();
    vprintln!(config, "Running projects in: {}", config.destination_path.display());
    let build_config = cargo_toml.get_build_config(&config.build_name).ok_or_else(|| anyhow::anyhow!("Build configuration not found"))?;
    println!("Building LP project...");
    build_lp(config, build_config, &filtered_env)?;
    println!("Running main project...");
    run_main(config, build_config, &filtered_env)?;
    println!("Run completed.");
    Ok(cargo_toml)
}

fn dont_copy_main(_path: &Path, _src: &Path, _dst: &Path) -> bool {
    false
}
fn dont_delete_main(_path: &Path, _src: &Path, _dst: &Path) -> bool {
    false
}

fn get_template_path(base_path: &Path) -> PathBuf {
    base_path.join("template")
}

fn gen_main_project(
    config: &Config, cargo_toml: &cargo_toml::CargoToml,
) -> Result<()> {
    project_clone::clone_project(&config.manifest, &config.get_destination_path(), &config.get_gen_double("main"),
        &get_template_path(&config.manifest), &config.get_gen_double("main"),
        dont_delete_main, dont_copy_main)?;
    let main_cargo_toml = cargo_toml.generate_main_file(&config.manifest)?.to_string();
    fs::write(&config.get_destination_main_full().join("Cargo.toml"), main_cargo_toml)?;
    Ok(())
}

fn dont_copy_lp(_path: &Path, _src: &Path, _dst: &Path) -> bool {
    false
}
fn dont_delete_lp(_path: &Path, _src: &Path, _dst: &Path) -> bool {
    false
}

fn gen_lp_project(
    config: &Config, cargo_toml: &cargo_toml::CargoToml,
) -> Result<()> {
    project_clone::clone_project(&config.manifest, &config.get_destination_path(), &config.get_gen_double("lp"),
        &get_template_path(&config.manifest), &config.get_gen_double("lp"),
        dont_delete_lp, dont_copy_lp)?;
    let lp_cargo_toml = cargo_toml.generate_lp_file(&config.manifest)?.to_string();
    fs::write(&config.get_destination_lp_full().join("Cargo.toml"), lp_cargo_toml)?;
    Ok(())
}
