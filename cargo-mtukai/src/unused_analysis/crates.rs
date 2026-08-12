use std::{collections::HashSet, path::Path};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_disabled_optional_deps_from_toml() {
        let toml_content = r#"
[dependencies]
esp-println = { version = "0.17", optional = true }
esp-alloc = { version = "0.10", optional = true }
esp-lp-hal = { version = "0.3", optional = true }
panic-halt = { version = "0.2", optional = true }

[features]
has-lp-core = ["dep:esp-println", "dep:esp-alloc"]
is-lp-core = ["dep:esp-lp-hal", "dep:panic-halt"]
"#;
        let disabled_lp = parse_disabled_optional_deps_from_toml(toml_content, &vec!["esp32c6".to_owned(),"is-lp-core".to_owned()]);
        assert!(disabled_lp.contains("esp_alloc"));
        assert!(disabled_lp.contains("esp_println"));
        assert!(!disabled_lp.contains("esp_lp_hal"));
        assert!(!disabled_lp.contains("panic_halt"));

        let disabled_hp = parse_disabled_optional_deps_from_toml(toml_content, &vec!["esp32c6".to_owned(),"has-lp-core".to_owned()]);
        assert!(!disabled_hp.contains("esp_alloc"));
        assert!(!disabled_hp.contains("esp_println"));
        assert!(disabled_hp.contains("esp_lp_hal"));
        assert!(disabled_hp.contains("panic_halt"));
    }
}

pub(super) fn collect_disabled_optional_deps(
    manifest_path: &Path,
    features: &Vec<String>,
) -> HashSet<String> {
    let manifest_file = if manifest_path.is_file() {
        manifest_path.to_path_buf()
    } else {
        manifest_path.join("Cargo.toml")
    };

    let Ok(content) = std::fs::read_to_string(&manifest_file) else {
        return HashSet::new();
    };

    parse_disabled_optional_deps_from_toml(&content, features)
}


fn parse_disabled_optional_deps_from_toml(
    toml_str: &str,
    features: &Vec<String>,
) -> HashSet<String> {
    let mut all_optional_deps = HashSet::new();
    let mut enabled_deps = HashSet::new();

    let Ok(doc) = toml_str.parse::<toml_edit::DocumentMut>() else {
        return HashSet::new();
    };

    let mut check_dep_table = |table: &toml_edit::Item| {
        if let Some(tbl) = table.as_table() {
            for (key, val) in tbl.iter() {
                let is_optional = if let Some(item_tbl) = val.as_table() {
                    item_tbl.get("optional").and_then(|v| v.as_bool()).unwrap_or(false)
                } else if let Some(item_inline) = val.as_inline_table() {
                    item_inline.get("optional").and_then(|v| v.as_bool()).unwrap_or(false)
                } else {
                    false
                };

                if is_optional {
                    all_optional_deps.insert(key.to_string());
                }
            }
        }
    };

    if let Some(deps) = doc.get("dependencies") {
        check_dep_table(deps);
    }
    if let Some(dev_deps) = doc.get("dev-dependencies") {
        check_dep_table(dev_deps);
    }
    if let Some(build_deps) = doc.get("build-dependencies") {
        check_dep_table(build_deps);
    }
    if let Some(target) = doc.get("target").and_then(|t| t.as_table()) {
        for (_k, v) in target.iter() {
            if let Some(deps) = v.get("dependencies") {
                check_dep_table(deps);
            }
        }
    }

    for feat in features.into_iter() {
        if all_optional_deps.contains(feat) {
            enabled_deps.insert(feat.to_string());
        }
    }

    let mut queue: Vec<String> = features.clone();
    if doc.get("features").and_then(|f| f.get("default")).is_some() && queue.is_empty() {
        queue.push("default".to_string());
    }

    let mut visited_features = HashSet::new();

    while let Some(feat_name) = queue.pop() {
        if !visited_features.insert(feat_name.clone()) {
            continue;
        }

        if all_optional_deps.contains(&feat_name) {
            enabled_deps.insert(feat_name.clone());
        }

        if let Some(feat_arr) = doc
            .get("features")
            .and_then(|f| f.get(&feat_name))
            .and_then(|v| v.as_array())
        {
            for item in feat_arr.iter() {
                if let Some(s) = item.as_str() {
                    let s = s.trim();
                    if let Some(dep_name) = s.strip_prefix("dep:") {
                        enabled_deps.insert(dep_name.to_string());
                    } else if s.contains("?/") {
                        // "foo?/bar" does not explicitly enable optional dep "foo"
                    } else if let Some((crate_name, _)) = s.split_once('/') {
                        if all_optional_deps.contains(crate_name) {
                            enabled_deps.insert(crate_name.to_string());
                        }
                        queue.push(crate_name.to_string());
                    } else {
                        if all_optional_deps.contains(s) {
                            enabled_deps.insert(s.to_string());
                        }
                        queue.push(s.to_string());
                    }
                }
            }
        }
    }

    let mut disabled = HashSet::new();
    for dep in all_optional_deps {
        let normalized = dep.replace('-', "_");
        if !enabled_deps.contains(&dep) && !enabled_deps.contains(&normalized) {
            disabled.insert(normalized);
        }
    }

    disabled
}
