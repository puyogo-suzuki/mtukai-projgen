use std::collections::{HashMap, HashSet};

use ra_ap_base_db::SourceDatabase;
use ra_ap_ide::RootDatabase;
use ra_ap_syntax::{
    ast::{self, AstNode, HasAttrs, HasModuleItem, HasName},
    AstToken,
};
use ra_ap_vfs::Vfs;

use super::{use_elimination, UseInfo, UnusedAnalysisResult, UnusedItem, UnusedItemKind};

/// Collected File Information in AST-level
#[derive(Debug)]
pub(super) struct AstCollectFile {
    pub(super) functions: HashMap<String, (ra_ap_syntax::ast::Fn, AstVisitInfo)>,
    pub(super) unresolved_imports: Vec<UseInfo>,
}

impl AstCollectFile {
    /// Returns a new `AstCollectFile` instance by collecting function information from the given `file`.
    pub(super) fn new_from_file(file: &ra_ap_syntax::SourceFile, unresolved_imports: Vec<UseInfo>, features: &[String]) -> AstCollectFile {
        fn visit_state(func: &ast::Fn, features: &[String]) -> AstVisitInfo {
            if has_panic_handler(func) || has_undead_macro(func, features) {
                AstVisitInfo::Visited
            } else {
                AstVisitInfo::None
            }
        }

        let mut functions = HashMap::new();
        fn insert_to_functions(functions: &mut HashMap<String, (ra_ap_syntax::ast::Fn, AstVisitInfo)>, func: &ra_ap_syntax::ast::Fn, features: &[String]) {
            functions.insert(get_ast_fn_key(&func), (func.clone(), visit_state(func, features)));
        }
        for item in file.items() {
            match item {
                ast::Item::Fn(func) => {
                    insert_to_functions(&mut functions, &func, features);
                }
                ast::Item::Impl(impl_) => {
                    // functions inside impl block
                    if let Some(assoc_item_list) = impl_.assoc_item_list() {
                        for assoc_item in assoc_item_list.assoc_items() {
                            if let ast::AssocItem::Fn(func) = assoc_item {
                                insert_to_functions(&mut functions, &func, features);
                            }
                        }
                    }
                }
                ast::Item::Trait(trait_) => {
                    // functions inside trait definition
                    if let Some(assoc_item_list) = trait_.assoc_item_list() {
                        for assoc_item in assoc_item_list.assoc_items() {
                            if let ast::AssocItem::Fn(func) = assoc_item && func.body().is_some() {
                                insert_to_functions(&mut functions, &func, features);
                            }
                        }
                    }
                }
                _ => {}
            }
        }  
        AstCollectFile { functions, unresolved_imports }
    }

    /// Converts `functions` into the `Vec` of `UnusedItem`s.
    pub(super) fn into_vec_unuseditem(&self) -> Vec<UnusedItem> {
        let mut ranges: Vec<UnusedItem> = self.functions.values().filter_map(|(func, visit)| {
            if let AstVisitInfo::Unused = visit {
                Some(UnusedItem::new(get_ast_fn_key(&func), func.syntax().text_range(), UnusedItemKind::Function))
            } else {
                None
            }
        }).collect();
        let mut res = use_elimination::into_vec_unuseditem(&self.unresolved_imports);
        res.append(&mut ranges);
        res.sort();
        res
    }
}

/// Visit Information for `AstCollect`
#[derive(Debug, Eq, PartialEq)]
pub(super) enum AstVisitInfo {
    /// It is not recognized in HIR-level.
    None,
    /// It is unused.
    Unused,
    /// It is used.
    Visited
}

/// Collected Information in AST-level
#[derive(Debug)]
pub(super) struct AstCollect {
    files : HashMap<ra_ap_vfs::FileId, AstCollectFile>,
}

impl AstCollect {
    pub(super) fn new() -> Self {
        Self { files: HashMap::new() }
    }
    pub(super) fn get_function_info<S: AsRef<str>>(&mut self, file_id: ra_ap_vfs::FileId, key: S) -> Option<&mut (ra_ap_syntax::ast::Fn, AstVisitInfo)> {
        self.files.get_mut(&file_id).and_then(|file_entry| file_entry.functions.get_mut(key.as_ref()))
    }
    pub(super) fn collect_from_file(&mut self, file_id : &ra_ap_vfs::FileId, db : &RootDatabase, root_krate : &ra_ap_hir::Crate, disabled_optional_deps: &HashSet<String>, features : &[String]) {
        let text = db.file_text(file_id.clone()).text(db);
        let parse = ra_ap_syntax::SourceFile::parse(&text, root_krate.edition(db));
        let file = parse.tree();

        self.files.insert(file_id.clone(), AstCollectFile::new_from_file(&file, use_elimination::unresolved_imports(db, file_id, root_krate, disabled_optional_deps), features));
    }

    pub(super) fn into_unused_analysis_result(self, vfs: &Vfs) -> UnusedAnalysisResult {
        let mut res = HashMap::new();
        for (path, file_entry) in
            self.files.into_iter().filter_map(|(file_id, file_entry)| {
                vfs.file_path(file_id).as_path().map(|v| v.to_path_buf()).map(|p| (p, file_entry))
            }) {
            let ranges = file_entry.into_vec_unuseditem();
            if !ranges.is_empty() {
                res.insert(path.into(), ranges);
            }
        }
        UnusedAnalysisResult { unuseds: res }
    }
}

/// Generate the identifier (key) for functions. e.g., impl:Foo::bar
pub(super) fn get_ast_fn_key(func: &ast::Fn) -> String {
    fn sanitize_type_str(s: &str) -> String {
        s.chars().filter(|c| !c.is_whitespace()).collect()
    }
    let fn_name = func.name().map(|n| n.text().to_string()).unwrap_or_else(|| "<unnamed>".to_string());
    let mut parts = Vec::new();

    let mut current = func.syntax().parent();
    while let Some(node) = current {
        if let Some(impl_) = ast::Impl::cast(node.clone()) {
            let mut thispart = "impl:".to_string();
            let self_ty = impl_
                .self_ty()
                .map(|t| sanitize_type_str(&t.syntax().text().to_string()))
                .unwrap_or_default();
            if let Some(tr) = impl_
                .trait_()
                .map(|t| sanitize_type_str(&t.syntax().text().to_string()))
            {
                thispart.push_str(&tr);
                thispart.push_str(":for:");
            }
            thispart.push_str(&self_ty);
            parts.push(thispart);
        } else if let Some(tr) = ast::Trait::cast(node.clone()) {
            if let Some(trait_name) = tr.name().map(|n| n.text().to_string()) {
                parts.push(format!("trait:{}", trait_name));
            }
        } else if let Some(m) = ast::Module::cast(node.clone()) {
            if let Some(mod_name) = m.name().map(|n| n.text().to_string()) {
                parts.push(format!("mod:{}", mod_name));
            }
        }
        current = node.parent();
    }

    parts.reverse();
    parts.push(format!("fn:{}", fn_name));
    parts.join("::")
}

// Check if the function has a `#[panic_handler]` attribute.
// panic_handlers are used.
fn has_panic_handler(func: &ast::Fn) -> bool {
    func.attrs().any(|attr| {
        attr.path().map(|p| p.syntax().text().to_string() == "panic_handler").unwrap_or(false)
    })
}

fn has_undead_macro(func: &ast::Fn, features: &[String]) -> bool {
    const MACRO_NAME: &str = "mtukai_projgen_undead";
    func.attrs().any(|attr| {
        if let Some(path) = attr.path() {
            if !path.segment().and_then(|s| s.name_ref()).map(|n| n.text() == MACRO_NAME).unwrap_or(false) && // #[mtukai_projgen_procmacro::mtukai_projgen_undead]
                !path.as_single_name_ref().map(|s| s.text() == MACRO_NAME).unwrap_or(false) { // #[mtukai_projgen_undead]
                return false;
            }
            if let Some(tt) = attr.meta().and_then(|t| match t { ast::Meta::TokenTreeMeta(t) => t.token_tree(), _ => None }) {
                // #[mtukai_projgen_procmacro::mtukai_projgen_undead("foo", "bar")]
                tt.token_trees_and_tokens()  
                    .filter_map(|it| it.into_token().and_then(ast::String::cast))  
                    .any(|s| {
                        let f = s.text().to_string();
                        let f = f.chars().skip(1).take(f.len() - 2).collect::<String>(); // Get Foo from "Foo".
                        features.contains(&f)
                    })
            } else {  // #[mtukai_projgen_procmacro::mtukai_projgen_undead]
                true // No features specified, so it is always undead.
            }
        } else {
            false // Unknown!
        }
    })
}


#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_simple_fns() {
        let code = "
            fn foo() {}
            fn bar() {}
            fn baz() {}
        ";
        let c = ra_ap_syntax::SourceFile::parse(code, ra_ap_syntax::Edition::DEFAULT);
        let mut ac = AstCollectFile::new_from_file(&c.tree(), vec![], &[]);
        assert!(ac.functions.iter().all(|(_, (fn_, _))| {
            let fname = fn_.name().map(|n| n.text().to_string()).unwrap_or_default();
            fname == "foo" || fname == "bar" || fname == "baz"
        }), "Function names should be 'foo', 'bar', and 'baz'");
        assert!(ac.functions.iter().all(|(f, _)| {
            f == "fn:foo" || f == "fn:bar" || f == "fn:baz"
        }), "Function keys should be 'fn:foo', 'fn:bar', and 'fn:baz'");
        assert!(ac.functions.iter().all(|(_, (_, visit))| {
            *visit == AstVisitInfo::None
        }), "All functions should have visit state 'None'");
        {
            let (_, v) = ac.functions.get_mut("fn:bar").unwrap();
            *v = AstVisitInfo::Visited;
            let (_, v) = ac.functions.get_mut("fn:baz").unwrap();
            *v = AstVisitInfo::Unused;
        }
        let unused_items = ac.into_vec_unuseditem();
        assert_eq!(unused_items.len(), 1, "There should be one unused item");
        assert_eq!(unused_items[0].key, "fn:baz", "Only 'baz' should be marked as unused");
    }

    #[test]
    fn test_impl_trait_fns() {
        let code = "
            struct MyStruct {}
            trait MyTrait {
                fn trait_fn(&self);
                fn default_fn(&self) {}
            }
            impl MyTrait for MyStruct {
                fn trait_fn(&self) {}
            }
            impl MyStruct {
                fn assoc_fn(&self) {}
            }
        ";
        let c = ra_ap_syntax::SourceFile::parse(code, ra_ap_syntax::Edition::DEFAULT);
        let ac = AstCollectFile::new_from_file(&c.tree(), vec![], &[]);
        assert!(ac.functions.iter().any(|(f, _)| f == "trait:MyTrait::fn:default_fn"), "Function 'default_fn' should be collected");
        assert!(ac.functions.iter().any(|(f, _)| f == "impl:MyTrait:for:MyStruct::fn:trait_fn"), "Function 'trait_fn' in impl should be collected");
        assert!(ac.functions.iter().any(|(f, _)| f == "impl:MyStruct::fn:assoc_fn"), "Function 'assoc_fn' in impl should be collected");
    }

    #[test]
    fn test_panic_handler() {
        let code = "
            #[panic_handler]
            fn foo() {}
            fn bar() {}
        ";
        let c = ra_ap_syntax::SourceFile::parse(code, ra_ap_syntax::Edition::DEFAULT);
        let mut tree = c.tree().items();
        let foo = match tree.next().unwrap() {
            ast::Item::Fn(func) => func,
            _ => panic!("Expected a function item"),
        };
        assert!(has_panic_handler(&foo), "Function 'foo' should have a panic_handler attribute");
        let bar = match tree.next().unwrap() {
            ast::Item::Fn(func) => func,
            _ => panic!("Expected a function item"),
        };
        assert!(!has_panic_handler(&bar), "Function 'bar' should not have a panic_handler attribute");
    }

    #[test]
    fn test_undead_macro() {
        let code = "
            #[mtukai_projgen_undead(\"feature1\")]
            fn foo() {}
            #[mtukai_projgen_undead]
            fn bar() {}
            fn baz() {}
            #[mtukai_projgen_undead(\"feature2\")]
            fn qux() {}
        ";
        let c = ra_ap_syntax::SourceFile::parse(code, ra_ap_syntax::Edition::DEFAULT);
        let mut tree = c.tree().items();
        let foo = match tree.next().unwrap() {
            ast::Item::Fn(func) => func,
            _ => panic!("Expected a function item"),
        };
        assert!(has_undead_macro(&foo, &["feature1".to_string()]), "Function 'foo' should have an undead macro with feature1");
        let bar = match tree.next().unwrap() {
            ast::Item::Fn(func) => func,
            _ => panic!("Expected a function item"),
        };
        assert!(has_undead_macro(&bar, &[]), "Function 'bar' should have an undead macro without features");
        let baz = match tree.next().unwrap() {
            ast::Item::Fn(func) => func,
            _ => panic!("Expected a function item"),
        };
        assert!(!has_undead_macro(&baz, &[]), "Function 'baz' should not have an undead macro");
        let qux = match tree.next().unwrap() {
            ast::Item::Fn(func) => func,
            _ => panic!("Expected a function item"),
        };
        assert!(!has_undead_macro(&qux, &["feature1".to_string()]), "Function 'qux' should not have an undead macro with feature1");
    }
}