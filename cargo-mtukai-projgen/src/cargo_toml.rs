use anyhow::{Context, Result};
use toml_edit::{DocumentMut};
use std::path::Path;

#[derive(Debug)]
pub struct BuildConfig {
    pub name : String,
    pub lp_target : Option<String>,
    pub lp_features : Option<String>,
    pub lp_args : Option<String>
}

pub struct CargoToml {
    doc: DocumentMut,
    name: String,
    build_configs: Vec<BuildConfig>
}

impl CargoToml {
    pub fn new<P : AsRef<Path>>(path: P) -> Result<CargoToml> {
        let content = std::fs::read_to_string(path)?;
        let doc : DocumentMut = content.parse()?;
        let name = doc["package"]["name"].as_str().context("Name is missing")?.to_string();

        let build_configs = Self::read_build_configs(&doc)?;

        Ok(CargoToml { doc, name, build_configs })
    }

    pub fn get_build_configs(&self) -> &Vec<BuildConfig> {
        &self.build_configs
    }

    pub fn get_build_config<S: AsRef<str>>(&self, name: S) -> Option<&BuildConfig> {
        self.build_configs.iter().find(|bc| bc.name == *name.as_ref())
    }

    fn read_build_configs(dm : &DocumentMut) -> Result<Vec<BuildConfig>> {
        let mut build_configs = Vec::new();
        if let Some(builds) = dm.get("package").and_then(|pkg| pkg.get("metadata"))
            .and_then(|meta| meta.get("mtukai")).and_then(|mtukai| mtukai.get("build")) {
            if let Some(array) = builds.as_array_of_tables() {
                for item in array.iter() {
                    let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("");
                    let lp_target = item.get("lp_target").and_then(|v| v.as_str()).map(|s| s.to_string());
                    let lp_features = item.get("lp_features").and_then(|v| v.as_str()).map(|s| s.to_string());
                    let lp_args = item.get("lp_args").and_then(|v| v.as_str()).map(|s| s.to_string());
                    build_configs.push(BuildConfig { name: name.to_string(), lp_target, lp_features, lp_args });
                }
            }
        }
        Ok(build_configs)
    }

    fn translate_dependencies_path(dep_table : &mut toml_edit::Table) {
        for (_, value) in dep_table.iter_mut() {
            if let Some(v_tbl) = value.as_inline_table_mut()
                && let Some(path_item) = v_tbl.get_mut("path")
                && let Some(path_str) = path_item.as_str()
                // Check if the path is relative.
                && Path::new(path_str).is_relative() {
                // Update the path to point to the main directory
                let new_path = format!("../../{}", path_str);
                *path_item = new_path.into();
            }
        }
    }

    pub fn generate_lp_file(&self) -> Result<DocumentMut> {
        // Implementation for generating LP file
        let mut lptoml = self.doc.clone();
        //
        // Prepare features.
        //
        if let Some(default_features) = lptoml["features"].get_mut("default")
            && let Some(ary) = default_features.as_array_mut() {
            ary.push("is-lp-core");
        } else {
            let mut ary = toml_edit::Array::new();
            ary.push("is-lp-core");
            lptoml["features"]["default"] = toml_edit::Item::from(ary);
        }
        //
        // Prepare dependencies.
        //
        // For dependencies that refer to the relative path,
        lptoml["dependencies"].as_table_mut().map(|deps| {
            Self::translate_dependencies_path(deps);
        });
        //
        // Remove the 'bin' section
        //
        lptoml.remove("bin");

        Ok(lptoml)
    }

    pub fn generate_main_file(&self) -> Result<DocumentMut> {
        // Implementation for generating main file
        let mut maintoml = self.doc.clone();
        if let Some(default_features) = maintoml["features"].get_mut("default")
            && let Some(ary) = default_features.as_array_mut() {
            ary.push("has-lp-core");
        } else {
            let mut ary = toml_edit::Array::new();
            ary.push("has-lp-core");
            maintoml["features"]["default"] = toml_edit::Item::from(ary);
        }
        if maintoml.get("package").is_none() {
            maintoml["package"] = toml_edit::Item::Table(toml_edit::Table::new());
        }
        if maintoml["package"].get("metadata").is_none() {
            maintoml["package"]["metadata"] = toml_edit::Item::Table(toml_edit::Table::new());
        }
        {
            let metadata = if let Some(metadata) = maintoml.get_mut("package")
                && let Some(metadata) = metadata.get_mut("metadata")
                && let Some(metadata2) = metadata.get_mut("mtukai")
                && let Some(metadata2) = metadata2.as_table_mut() {
                metadata2
            } else {
                maintoml["package"]["metadata"]["mtukai"] = toml_edit::Item::Table(toml_edit::Table::new());
                maintoml["package"]["metadata"]["mtukai"].as_table_mut().unwrap()
            };
            metadata["lp_path"] = toml_edit::Item::from(Path::new("..").join("lp").to_str().unwrap_or_default());
        }
        maintoml["dependencies"].as_table_mut().map(|deps: &mut toml_edit::Table| {
            Self::translate_dependencies_path(deps);
        });
        Ok(maintoml)
    }
}