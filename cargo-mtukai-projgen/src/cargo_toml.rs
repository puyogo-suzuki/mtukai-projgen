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

    pub fn generate_lp_file(&self) -> Result<DocumentMut> {
        // Implementation for generating LP file
        let mut lptoml = self.doc.clone();
        let mut ary = toml_edit::Array::new();
        ary.push("is-lp-core");
        lptoml["features"]["default"] = toml_edit::Item::from(ary);
        lptoml.remove("bin");
        Ok(lptoml)
    }

    pub fn generate_main_file(&self) -> Result<DocumentMut> {
        // Implementation for generating main file
        let mut maintoml = self.doc.clone();
        Ok(maintoml)
    }
}