use anyhow::{Context, Result};
use toml_edit::{DocumentMut};
use std::path::Path;

pub struct CargoToml {
    doc: DocumentMut,
    name: String
}

impl CargoToml {
    pub fn new<P : AsRef<Path>>(path: P) -> Result<CargoToml> {
        let content = std::fs::read_to_string(path)?;
        let doc : DocumentMut = content.parse()?;
        let name = doc["package"]["name"].as_str().context("Name is missing")?.to_string();
        Ok(CargoToml { doc, name })
    }

    fn translate_dependencies_path(dep_table : &mut toml_edit::Table) {
        for (key, value) in dep_table.iter_mut() {
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
        if let Some(default_features) = lptoml["features"].get_mut("default")
            && let Some(ary) = default_features.as_array_mut() {
            ary.push("is-lp-core");
        } else {
            let mut ary = toml_edit::Array::new();
            ary.push("is-lp-core");
            lptoml["features"]["default"] = toml_edit::Item::from(ary);
        }
        // For dependencies that refer to the relative path,
        lptoml["dependencies"].as_table_mut().map(|deps| {
            Self::translate_dependencies_path(deps);
        });
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
        maintoml["dependencies"].as_table_mut().map(|deps| {
            Self::translate_dependencies_path(deps);
        });
        Ok(maintoml)
    }
}