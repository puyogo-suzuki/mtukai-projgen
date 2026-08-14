use std::collections::HashSet;

use ra_ap_base_db::SourceDatabase;
use ra_ap_hir::{Crate, Semantics};
use ra_ap_ide::{RootDatabase, TextRange};
use ra_ap_syntax::{algo, ast::UseTree, AstNode, SyntaxKind};
use ra_ap_vfs::FileId;

#[derive(Debug)]
pub(crate) struct ImportTree {
    pub(crate) syntax: ra_ap_syntax::SyntaxNode,
    pub(crate) text: Option<String>,
    pub(crate) leafs: Vec<ImportTree>,
}

#[derive(Debug)]
pub(crate) struct UseInfo {
    pub(crate) syntax: ra_ap_syntax::SyntaxNode,
    pub(crate) import_tree: Option<ImportTree>,
}

pub(crate) fn collect_imports(file : &ra_ap_syntax::SourceFile) -> Vec<UseInfo> {
    fn go(ut: &UseTree) -> ImportTree {
        let mut leafs = vec![];
        if let Some(utl) = ut.use_tree_list() {
            for ut in utl.use_trees() {
                leafs.push(go(&ut));
            }
        }
        ImportTree {
            syntax: ut.syntax().clone(),
            text: ut.path().map(|p| p.to_string()),
            // unknown appears when:
            // use { foo, bar }; for the top level.
            leafs
        }
    }
    let mut v = vec![];
    for u in file.syntax().descendants().filter_map(ra_ap_syntax::ast::Use::cast) {
        v.push(UseInfo {
            syntax: u.syntax().clone(),
            import_tree: u.use_tree().map(|ut| go(&ut))
        });
    }
    v
}

fn text_range_with_trailing_comma(syntax: &ra_ap_syntax::SyntaxNode) -> TextRange {
    algo::next_non_trivia_token(syntax.clone())
        .filter(|token| token.kind() == SyntaxKind::COMMA)
        .map(|token| TextRange::new(syntax.text_range().start(), token.text_range().end()))
        .unwrap_or_else(|| syntax.text_range())
}

fn get_top_level_name(txt: &str) -> &str {
    txt.split("::").next().unwrap_or(txt)
}

pub(super) fn unresolved_imports(
    db: &RootDatabase,
    file: &FileId,
    root_krate: &Crate,
    disabled_optional_deps: &HashSet<String>,
) -> Vec<UseInfo> {
    let sema = Semantics::new(db);
    let module : Vec<_> = sema.file_to_module_defs(*file).collect();
    if module.is_empty() {
        return Vec::new();
    }
    let module = module[0];
    let mut diagnostics_acc = Vec::new();
    module.diagnostics(db, &mut diagnostics_acc, false);
    let unresolveds : HashSet<_> = diagnostics_acc
        .into_iter()
        .filter_map(|diagnostic| match diagnostic {
            ra_ap_hir::AnyDiagnostic::UnresolvedImport(imp) => Some((*imp).decl.value.text_range()),
            _ => None,
        }).collect();

    if unresolveds.is_empty() {
        return Vec::new();
    }
    let text = db.file_text(file.clone()).text(db);
    let parse: ra_ap_syntax::Parse<ra_ap_syntax::SourceFile> = ra_ap_syntax::SourceFile::parse(&text, root_krate.edition(db));
    let file = parse.tree();
    let imps = collect_imports(&file);

    fn go(
        imp: ImportTree,
        unresolveds: &HashSet<TextRange>,
        disabled_optional_deps: &HashSet<String>,
        is_toplevel: bool
    ) -> Option<ImportTree> {
        let ImportTree { syntax, text, leafs } = imp;
        let is_disabled_dep = is_toplevel && text.as_ref().map(|t| disabled_optional_deps.contains(get_top_level_name(t))).unwrap_or(false);
        if unresolveds.contains(&syntax.text_range()) || is_disabled_dep {
            Some(ImportTree { syntax, text, leafs: vec![] })
        } else {
            let mut v = vec![];
            let leafs_len = leafs.len();
            let text_is_some = text.is_some();
            for i in leafs {
                if let Some(unresolved) = go(i, unresolveds, disabled_optional_deps, text.is_none() && is_toplevel) {
                    v.push(unresolved);
                }
            }
            if v.is_empty() { // If all leafs are resolved, return None to indicate that this import tree is fully resolved.
                None
            } else {
                Some(ImportTree {
                    syntax,
                    text,
                    // Comment out the whole if all leafs are unused.
                    // However annonymous leafs (like `use { foo, bar };`) are not commented out even if all leafs are unused.
                    leafs: if leafs_len == v.len() && text_is_some {vec![]} else {v}
                })
            }
        }
    }

    imps.into_iter()
        .filter_map(|UseInfo { syntax: syn, import_tree: it }| {
            it.and_then(|it| go(it, &unresolveds, disabled_optional_deps, true))
                .map(|it| UseInfo { syntax: syn, import_tree: Some(it) })
        })
        .collect::<Vec<_>>()
}

fn concat_path<S: AsRef<str>>(base: S, child: &Option<String>) -> String {
    if let Some(p) = child {
        if base.as_ref().is_empty() {
            p.clone()
        } else {
            format!("{}::{}", base.as_ref(), p)
        }
    } else {
        base.as_ref().into()
    }
}

pub(super) fn from_cache_file(file_id: &ra_ap_syntax::SourceFile, unuseds: &HashSet<&str>) -> Vec<UseInfo> {
    let imps = collect_imports(file_id);
    fn go<S : AsRef<str>>(imp: ImportTree, unuseds: &HashSet<&str>, s: S) -> Option<ImportTree> {
        let cur_path = concat_path(s, &imp.text);
        let ImportTree { syntax, text, leafs } = imp;
        if unuseds.contains(cur_path.as_str()) {
            Some(ImportTree { syntax, text, leafs: vec![] })
        } else {
            let mut v = vec![];
            let leafs_len = leafs.len();
            let text_is_some = text.is_some();
            for i in leafs {
                if let Some(unresolved) = go(i, unuseds, &cur_path) {
                    v.push(unresolved);
                }
            }
            if v.is_empty() {
                None
            } else {
                Some(ImportTree {
                    syntax,
                    text,
                    // Comment out the whole if all leafs are unused.
                    // However annonymous leafs (like `use { foo, bar };`) are not commented out even if all leafs are unused.
                    leafs: if leafs_len == v.len() && text_is_some {vec![]} else {v}
                })
            }
        }
    }

    imps.into_iter()
        .filter_map(|UseInfo { syntax: syn, import_tree: it }| 
            it.and_then(|it| go(it, &unuseds, "")).map(|it|  UseInfo{ syntax: syn, import_tree: Some(it)})
        ).collect()
}

pub(super) fn into_vec_unuseditem(uses: &[UseInfo]) -> Vec<super::UnusedItem> {
    let mut s = vec![];
    for u in uses {
        fn go<S: AsRef<str>>(imp: &ImportTree, s: S, res: &mut Vec<super::UnusedItem>) {
            let cur_path = concat_path(s, &imp.text);
            if imp.leafs.is_empty() {
                res.push(super::UnusedItem::new(cur_path, text_range_with_trailing_comma(&imp.syntax), super::UnusedItemKind::Use));
            } else {
                for leaf in &imp.leafs {
                    go(leaf, &cur_path, res);
                }
            }
        }
        if let Some(import_tree) = &u.import_tree {
            if import_tree.leafs.is_empty() {
                if let Some(cur_path) = &import_tree.text {
                    s.push(super::UnusedItem::new(cur_path.clone(), text_range_with_trailing_comma(&u.syntax), super::UnusedItemKind::Use));
                }
            } else {
                go(import_tree, "", &mut s);
            }
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_top_level_name() {
        assert_eq!(get_top_level_name("cargo_toml::CargoToml"), "cargo_toml");
        assert_eq!(get_top_level_name("serde"), "serde");
    }

    #[test]
    fn test_concat_path() {
        assert_eq!(concat_path("", &Some("foo".into())), "foo");
        assert_eq!(concat_path("foo", &Some("bar".into())), "foo::bar");
        assert_eq!(concat_path("foo", &None), "foo");
    }
}