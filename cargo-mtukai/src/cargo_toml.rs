use anyhow::{Context, Result};
use toml_edit::{DocumentMut};
use std::path::Path;
use crate::chip_dic::get_conf_by_chip_name;

#[derive(Debug)]
pub struct BuildConfig {
    pub name : String,
    pub template_name : String,
    pub lp_target : Option<String>,
    pub lp_features : Option<String>,
    pub lp_args : Option<String>,
    pub lp_release : bool
}

pub struct CargoToml {
    doc: DocumentMut,
    name: String,
    build_configs: Vec<BuildConfig>,
}

impl CargoToml {
    pub fn new<P : AsRef<Path>>(path: P) -> Result<CargoToml> {
        let content = std::fs::read_to_string(path)?;
        let doc : DocumentMut = content.parse()?;
        let name = doc["package"]["name"].as_str().context("Name is missing")?.to_string();

        let build_configs = Self::read_build_configs(&doc)?;

        Ok(CargoToml { doc, name, build_configs })
    }

    pub fn get_build_config<S: AsRef<str>>(&self, name: S) -> Option<&BuildConfig> {
        self.build_configs.iter().find(|bc| bc.name == *name.as_ref())
    }

    fn read_build_configs(dm : &DocumentMut) -> Result<Vec<BuildConfig>> {
        let mut build_configs = Vec::new();
        if let Some(builds) = dm.get("package").and_then(|pkg| pkg.get("metadata"))
            .and_then(|meta| meta.get("mtukai")).and_then(|mtukai| mtukai.get("build")).and_then(|b| b.as_array_of_tables()) {
            for item in builds.iter() {
                let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("default");
                let chip_conf = item.get("chip").and_then(|v| v.as_str()).and_then(|chipname| get_conf_by_chip_name(chipname));
                let (lp_target, lp_features, lp_args, template_name) = chip_conf.map(|chip_conf| {
                    (Some(chip_conf.lp_target.to_owned()), Some(chip_conf.lp_features.to_owned()), Some(chip_conf.lp_args.to_owned()), Some(chip_conf.template.to_owned()))
                }).unwrap_or((None, None, None, None));
                let lp_target = item.get("lp_target").and_then(|v| v.as_str()).map(|s| s.to_string()).or(lp_target);
                let lp_features = item.get("lp_features").and_then(|v| v.as_str()).map(|s| s.to_string()).or(lp_features);
                let lp_args = item.get("lp_args").and_then(|v| v.as_str()).map(|s| s.to_string()).or(lp_args);
                let lp_release = item.get("lp_release").and_then(|v| v.as_bool()).unwrap_or(true);
                let template_name = item.get("template").and_then(|v| v.as_str()).map(|s| s.to_string()).or(template_name).unwrap_or(name.to_string());
                build_configs.push(BuildConfig { name: name.to_string(), template_name, lp_target, lp_features, lp_args, lp_release });
            }
        }
        Ok(build_configs)
    }

    fn translate_dependencies_path(dep_table : &mut toml_edit::Table) {
        for (_, value) in dep_table.iter_mut() {
            if let Some(path_item) = value.as_inline_table_mut().and_then(|tbl| tbl.get_mut("path"))
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
        let default_features = lptoml.entry("features").or_insert_with(|| toml_edit::Item::Table(toml_edit::Table::new()))
            .as_table_mut().expect("toml_edit has a bug.")
            .entry("default").or_insert_with(|| toml_edit::Item::Value(toml_edit::Value::Array(toml_edit::Array::new())))
            .as_array_mut().expect("toml_edit has a bug.");
        default_features.push("is-lp-core");
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
        let default_features = maintoml.entry("features").or_insert_with(|| toml_edit::Item::Table(toml_edit::Table::new()))
            .as_table_mut().expect("toml_edit has a bug.")
            .entry("default").or_insert_with(|| toml_edit::Item::Value(toml_edit::Value::Array(toml_edit::Array::new())))
            .as_array_mut().expect("toml_edit has a bug.");
        default_features.push("has-lp-core");

        let metadata =
            maintoml.entry("package").or_insert_with(|| toml_edit::Item::Table(toml_edit::Table::new()))
                    .as_table_mut().expect("toml_edit has a bug.")
                    .entry("metadata").or_insert_with(|| toml_edit::Item::Table(toml_edit::Table::new()))
                    .as_table_mut().expect("toml_edit has a bug.")
                    .entry("mtukai").or_insert_with(|| toml_edit::Item::Table(toml_edit::Table::new()))
                    .as_table_mut().expect("toml_edit has a bug.");
        metadata["lp_path"] = toml_edit::Item::from(Path::new("..").join("lp").to_str().unwrap_or_default());
        maintoml["dependencies"].as_table_mut().map(|deps: &mut toml_edit::Table| {
            Self::translate_dependencies_path(deps);
        });
        Ok(maintoml)
    }
}