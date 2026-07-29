use anyhow::{Context, Result};
use toml_edit::{DocumentMut};
use std::path::{Path, PathBuf};
use crate::chip_dic::get_conf_by_chip_name;

/// Return `entry` as a mutable `toml_edit::Table`.
/// If it is not a table, returns a error.
/// If it is empty, creates a new table and returns it.
fn get_table_or_create<'a>(entry: toml_edit::Entry<'a>) -> Result<&'a mut toml_edit::Table> {
    entry.or_insert_with(|| toml_edit::Item::Table(toml_edit::Table::new()))
        .as_table_mut().context("Unexpected non-table value in Cargo.toml.")
}

/// Build parameters given to cargo.
#[derive(Debug)]
pub struct BuildParameter {
    /// Target triple
    pub target : Option<String>,
    /// Enabled features
    pub features : Option<String>,
    /// Additional arguments to cargo
    pub args : Option<String>,
    /// Is release build?
    pub release : bool
}

/// Build configuration
#[derive(Debug)]
pub struct BuildConfig {
    /// Build configuration name
    pub name : String,
    /// Template name for ./template/<template_name>/{lp, main}
    pub template_name : String,
    /// Build parameters for LP coprocessor project
    pub lp_params : BuildParameter,
    /// Build parameters for main processor project
    pub main_params : BuildParameter
}

/// Summerized representation of Cargo.toml file.
pub struct CargoToml {
    /// Raw Cargo.toml document
    doc: DocumentMut,
    /// Crate name
    name: String,
    /// Rust Edition
    edition: String,
    /// Unused elimination flag. If true, the unused code will be eliminated during the build process.
    pub enable_unused_elimination: bool,
    /// Build configurations defined in Cargo.toml
    /// The build configurations are defined in the `[[package.metadata.mtukai.build]]` section of Cargo.toml.
    build_configs: Vec<BuildConfig>,
}

impl CargoToml {
    /// Create a new `CargoToml` instance from the given path to Cargo.toml file.
    /// This function reads the Cargo.toml file, parses it, and extracts the crate name and build configurations.
    /// It returns error if the toml does not contains the crate name or if the metadata of the mtukai is not valid.
    pub fn new<P : AsRef<Path>>(path: P) -> Result<CargoToml> {
        let content = std::fs::read_to_string(path)?;
        let doc : DocumentMut = content.parse()?;
        let name = doc["package"]["name"].as_str().context("Name is missing")?.to_string();
        let edition = doc["package"]["edition"].as_str().unwrap_or("2024").to_string();
        let enable_unused_elimination = doc["package"]["metadata"]["mtukai"]["unused_elimination"].as_bool().unwrap_or(false);
        let build_configs = Self::read_build_configs(&doc)?;
        Ok(CargoToml { doc, name, edition, enable_unused_elimination, build_configs })
    }

    /// Get the build configuration by name.
    /// Returns `None` if the build configuration with the given name does not exist.
    pub fn get_build_config<S: AsRef<str>>(&self, name: S) -> Option<&BuildConfig> {
        self.build_configs.iter().find(|bc| bc.name == *name.as_ref())
    }

    /// Read build parameters from the `[[package.metadata.mtukai.build]]` section of Cargo.toml.
    /// `release_default` is 
    fn read_build_parameters(item: Option<&toml_edit::InlineTable>, chip_conf: &Option<crate::chip_dic::ChipConfParams>, release_default : bool) -> BuildParameter {
        // Check if the `chip` is available.
        let (target, features, args) = match chip_conf {
            Some(crate::chip_dic::ChipConfParams { target, features, args }) => (Some(target.to_owned().to_owned()), Some(features.to_owned().to_owned()), Some(args.to_owned().to_owned())),
            None => (None, None, None)
        };
        // Check the inline table is available.
        if let Some(item) = item {
            let target = item.get("target").and_then(|v| v.as_str()).map(|s| s.to_string()).or(target);
            let features = item.get("features").and_then(|v| v.as_str()).map(|s| s.to_string()).or(features);
            let args = item.get("args").and_then(|v| v.as_str()).map(|s| s.to_string()).or(args);
            let release = item.get("release").and_then(|v| v.as_bool()).unwrap_or(release_default);
            BuildParameter { target, features, args, release }
        } else {
            BuildParameter { target, features, args, release: release_default }
        }
    }

    /// Read build configurations from the `[[package.metadata.mtukai.build]]` section of Cargo.toml.
    /// Returns a vector of `BuildConfig` instances.
    fn read_build_configs(dm : &DocumentMut) -> Result<Vec<BuildConfig>> {
        let mut build_configs = Vec::new();
        if let Some(builds) = dm.get("package").and_then(|pkg| pkg.get("metadata"))
            .and_then(|meta| meta.get("mtukai")).and_then(|mtukai| mtukai.get("build")).and_then(|b| b.as_array_of_tables()) {
            for item in builds.iter() {
                let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("default");
                let (chipconf_main, chipconf_lp, chipconf_template) = if let Some(v) = item.get("chip").and_then(|v| v.as_str()).and_then(|chipname| get_conf_by_chip_name(chipname)) {
                    (Some(v.main), Some(v.lp), Some(v.template))
                } else {
                    (None, None, None)
                };
                let lp_params = Self::read_build_parameters(item.get("lp").and_then(|v| v.as_inline_table()), &chipconf_lp, true);
                let main_params = Self::read_build_parameters(item.get("main").and_then(|v| v.as_inline_table()), &chipconf_main, false);
                let template_name = item.get("template").and_then(|v| v.as_str()).map(|s| s.to_string())
                    .or(chipconf_template.map(|t| t.to_owned())).unwrap_or(name.to_string());
                build_configs.push(BuildConfig { name: name.to_string(), template_name, lp_params, main_params });
            }
        }
        Ok(build_configs)
    }

    /// Translates the relative path to the dependencies.
    fn translate_dependencies_path(dep_table : &mut toml_edit::Table, original_path: &PathBuf) -> Result<()> {
        for (k, value) in dep_table.iter_mut() {
            if let Some(path_item) = value.as_inline_table_mut().and_then(|tbl| tbl.get_mut("path"))
                && let Some(path_str) = path_item.as_str()
                // Check if the path is relative.
                && Path::new(path_str).is_relative() {
                // Update the path to point to the main directory
                let new_path = original_path.join(path_str).canonicalize().with_context(|| format!("Failed to resolve path for the library {}", k.get()))?;
                *path_item = new_path.to_str().with_context(|| format!("Failed to resolve path for the library {}", k.get()))?.into();
            }
        }
        Ok(())
    }

    /// Set default features.
    fn prepare_feature(toml_doc: &mut toml_edit::Table, feature_name: &str) -> Result<()> {
        get_table_or_create(toml_doc.entry("features"))?.entry("default")
            .or_insert_with(|| toml_edit::Item::Value(toml_edit::Value::Array(toml_edit::Array::new())))
            .as_array_mut().context("Unexpected non-array value in Cargo.toml.")?.push(feature_name);
        Ok(())
    }

    /// Update the dependencies path in the Cargo.toml document.
    fn prepare_dependencies(toml_doc: &mut toml_edit::Table, original_path: &PathBuf) -> Result<()> {
        if let Some(deps) = toml_doc["dependencies"].as_table_mut() {
            Self::translate_dependencies_path(deps, original_path).with_context(|| "Failed to translate dependencies path")?;
        }
        Ok(())
    }

    /// Generate the Cargo.toml for the LP coprocessor project.
    pub fn generate_lp_file(&self, original_path: &PathBuf) -> Result<DocumentMut> {
        // Implementation for generating LP file
        let mut lptoml = self.doc.clone();
        
        Self::prepare_feature(&mut lptoml, "is-lp-core")?;
        Self::prepare_dependencies(&mut lptoml, original_path)?;
        //
        // Remove the 'bin' section
        //
        lptoml.remove("bin");

        Ok(lptoml)
    }

    /// Generate the Cargo.toml for the main processor project.
    pub fn generate_main_file(&self, original_path: &PathBuf) -> Result<DocumentMut> {
        // Implementation for generating main file
        let mut maintoml = self.doc.clone();
        Self::prepare_feature(&mut maintoml, "has-lp-core")?;
        Self::prepare_dependencies(&mut maintoml, original_path)?;

        let metadata =
            get_table_or_create(get_table_or_create(get_table_or_create(
                maintoml.entry("package"))?.entry("metadata"))?.entry("mtukai"))?;
        metadata["lp_path"] = toml_edit::Item::from(Path::new("..").join("lp").to_str().unwrap_or_default());
        Ok(maintoml)
    }
}