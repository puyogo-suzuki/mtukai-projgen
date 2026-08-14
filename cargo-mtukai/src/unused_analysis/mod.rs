use std::{
    collections::{HashMap, HashSet},
    fmt::Debug,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use ra_ap_base_db::SourceDatabase;
use ra_ap_hir::Crate;
use ra_ap_ide::{RootDatabase, Semantics, TextRange};
use ra_ap_load_cargo::{load_workspace_at, LoadCargoConfig, ProcMacroServerChoice};
use ra_ap_project_model::{CargoConfig, CargoFeatures};
use ra_ap_vfs::VfsPath;

use self::use_elimination::UseInfo;

mod use_elimination;
mod ast_collect;
mod hir_collect;
mod crates;

// fn textrange_to_line_column<S: AsRef<str>, T: AsRef<str>>(path_str: S, txt : T, range: ra_ap_ide::TextRange) -> String {
//     let start_line_col = txt.as_ref()[..<usize>::from(range.start())].lines().count();
//     let end_line_col = txt.as_ref()[..range.end().into()].lines().count();
//     // format!("{} {}:{:?}-{}:{:?}", path_str.as_ref(), start_line_col + 1, range.start(), end_line_col + 1, range.end())
//     format!("{}:{}-{}", path_str.as_ref(), start_line_col + 1, end_line_col + 1)
// }

/// Aanalyze to detect the unused items.
/// `features` must be comma-separated list of features to enable. If empty, all features are enabled.
/// `entry_point_name` is the name of the entry point function. If None, the main function is used as the entry point.
pub fn analyze_unused<S1: AsRef<str> + Debug, S2: AsRef<str>>(manifest_path: &Path, target: &Option<S1>, features: &Vec<String>, entry_point_name : Option<S2>) -> Result<UnusedAnalysisResult> {
    let root_crate_root = if manifest_path.is_file() {
        manifest_path.parent().unwrap_or(manifest_path)
    } else {
        manifest_path
    };
    let workspace_root = root_crate_root
        .canonicalize()
        .context("failed to canonicalize workspace root")?;

    let all_targets = target.is_none();
    let cargo_config = CargoConfig {
        sysroot: Some(ra_ap_project_model::RustLibSource::Discover),
        target: target.as_ref().map(|t| t.as_ref().to_string()),
        all_targets,
        features: if features.is_empty() {
            CargoFeatures::All
        } else {
            CargoFeatures::Selected {
                features: features.clone(),
                no_default_features: false,
            }
        },
        ..Default::default()
    };

    let load_config = LoadCargoConfig {
        load_out_dirs_from_check: true,
        with_proc_macro_server: ProcMacroServerChoice::Sysroot,
        prefill_caches: true,
        num_worker_threads: std::thread::available_parallelism().map(|parallelism| parallelism.get()).unwrap_or(1),
        proc_macro_processes: 1,
    };
    let (root_db, vfs, _) = load_workspace_at(&workspace_root.as_path(), &cargo_config, &load_config, &|_| {})?;
    let workspace_root = VfsPath::new_real_path(
        workspace_root.to_str().ok_or_else(|| anyhow::anyhow!("failed to convert workspace root to string"))?.to_owned());
    let bin_root = workspace_root.join("src").and_then(|p| p.join("bin")).and_then(|p| p.join("main.rs")).and_then(|p| vfs.file_id(&p)).context("The rust root source file is not found")?.0;
    let root_krate = Crate::all(&root_db).into_iter().find(|c| c.root_file(&root_db) == bin_root).context("not found ")?;

    let source_root = root_db.source_root(
        root_db.file_source_root(root_krate.root_file(&root_db)).source_root_id(&root_db)
    ).source_root(&root_db);

    let gen_dir = workspace_root.join("generated");
    let tem_dir = workspace_root.join("template");
    // for file_id in source_root.iter() {
    //     if let Some(path) = source_root.path_for_file(&file_id) { 
    //         // exclude template and generated directory.
    //         if (gen_dir.as_ref().map(|d| !path.starts_with(&d)).unwrap_or(false) && tem_dir.as_ref().map(|d| !path.starts_with(&d)).unwrap_or(false))
    //         && let Some((_, Some("rs"))) = path.name_and_extension() {  
    //             println!("----- file_id: {:?}, path: {:?} ------", file_id, path);
    //             let text = root_db.file_text(file_id).text(&root_db);  
    //             let parse = ra_ap_syntax::SourceFile::parse(&text, root_krate.edition(&root_db));  
    //             let file = parse.tree();  
    //             for item in file.items() {  
    //                 match item {  
    //                     ast::Item::Fn(func) => {  
    //                         println!("function {} at {:?}", func.name().map(|n| n.text().to_string()).unwrap_or_else(|| "<unnamed>".to_string()), textrange_to_line_column(path.as_path().unwrap().as_str(), &text, func.syntax().text_range()));
    //                     }  
    //                     ast::Item::Impl(impl_) => {  
    //                         // implブロック内の関数  
    //                         if let Some(assoc_item_list) = impl_.assoc_item_list() {  
    //                             for assoc_item in assoc_item_list.assoc_items() {  
    //                                 if let ast::AssocItem::Fn(func) = assoc_item {  
    //                                     println!("impl function {} at {:?}", func.name().map(|n| n.text().to_string()).unwrap_or_else(|| "<unnamed>".to_string()), textrange_to_line_column(path.as_path().unwrap().as_str(), &text, func.syntax().text_range()));
    //                                 }
    //                             }
    //                         }
    //                     }
    //                     ast::Item::Trait(trait_) => {  
    //                         // トレイト定義内の関数  
    //                         if let Some(assoc_item_list) = trait_.assoc_item_list() {  
    //                             for assoc_item in assoc_item_list.assoc_items() {  
    //                                 if let ast::AssocItem::Fn(func) = assoc_item {
    //                                     if func.body().is_some() {
    //                                         println!("trait function {} at {:?}", func.name().map(|n| n.text().to_string()).unwrap_or_else(|| "<unnamed>".to_string()), textrange_to_line_column(path.as_path().unwrap().as_str(), &text, func.syntax().text_range()));
    //                                     }
    //                                 }
    //                             }
    //                         }
    //                     }
    //                     _ => {}  
    //                 }  
    //             }  
    //             println!("----------")
    //         }  
    //     }  
    // }

    // let mut callgraph = CallGraph {
    //     nodes: std::collections::HashSet::new(),
    //     edges: std::collections::HashMap::new(),
    // };
    let hircollect = ra_ap_hir::attach_db(&root_db, || {
        let collections = hir_collect::HirCollect::new(&root_krate, &root_db, entry_point_name)?;
        // println!("Result: {:?}", collections);
        collections.walk_dependency(&root_db);
        Some(collections)
    }).context("Failed to search the main function")?;

    let disabled_optional_deps = crates::collect_disabled_optional_deps(manifest_path, features);
    let mut ast_collect = ast_collect::AstCollect::new();

    ra_ap_hir::attach_db(&root_db, || {
        for file_id in source_root.iter() {
            if let Some(path) = source_root.path_for_file(&file_id)
                // exclude template and generated directory.
                && gen_dir.as_ref().map(|d| !path.starts_with(d)).unwrap_or(false)
                && tem_dir.as_ref().map(|d| !path.starts_with(d)).unwrap_or(false)
                && let Some((_, Some("rs"))) = path.name_and_extension() {
                ast_collect.collect_from_file(&file_id,&root_db,&root_krate,&disabled_optional_deps,features);
            }
        }
    });

    check_used_functions_in_ast(&hircollect, &mut ast_collect, &root_db);

    // for (file_id, fi) in ast_collect.files.iter() {
    //     for (_, (f, visit)) in fi.functions.iter() {
    //         let path = vfs.file_path(*file_id).as_path().map(|v| v.to_string()).unwrap_or_default();
    //         let txt = root_db.file_text(*file_id).text(&root_db);
    //         println!("function {} at {:?} is {:?}", f.name().map(|n| n.text().to_string()).unwrap_or_else(|| "<unnamed>".to_string()), textrange_to_line_column(path, txt, f.syntax().text_range()), visit);
    //     }
    // }

    // for (f, vi) in hircollect.functions.iter() {
    //     println!("hir function {} is {}", f.name(&root_db).display(&root_db, ra_ap_ide::Edition::Edition2024), vi.is_visited());
    // }

    // let mut dotout = std::fs::File::create("def_use_graph.dot").context("failed to create def_use_graph.dot")?;
    // dotout.write(b"digraph G {\n").context("failed to write to def_use_graph.dot")?;
    // fn get_node_name(f: &ra_ap_hir::Function) -> String {
    //     format!("{:?}", f).replace("(", "_").replace(")", "_")
    // }
    // for n in callgraph.nodes.iter() {
    //     let name = n.name(&root_db);
    //     if n.module(&root_db).krate(&root_db) == root_krate {
    //         dotout.write(&format!("{} [label = \"{}\", fillcolor=4];\n", get_node_name(n) , name.display(&root_db, ra_ap_ide::Edition::Edition2024)).into_bytes()).unwrap();
    //     } else {
    //         dotout.write(&format!("{} [label = \"{}\"];\n", get_node_name(n) , name.display(&root_db, ra_ap_ide::Edition::Edition2024)).into_bytes()).unwrap();
    //     }
    // }
    // for (caller, callees) in callgraph.edges.iter() {
    //     for callee in callees {
    //         dotout.write(&format!("{} -> {};\n", get_node_name(caller), get_node_name(callee)).into_bytes()).unwrap();
    //     }
    // }
    // dotout.write(b"}\n").context("failed to write to def_use_graph.dot")?;
    Ok(ast_collect.into_unused_analysis_result(&vfs))
}

fn check_used_functions_in_ast(hircollect: &hir_collect::HirCollect, ast_collect: &mut ast_collect::AstCollect, db: &RootDatabase) {
    let sema = Semantics::new(db);
    for (f, visit) in hircollect.functions.iter() {
        if let Some(source) = sema.source(f.clone()) {
            let file_id = if let Some(mf) = source.file_id.macro_file() {
                mf.loc(db).kind.original_call_range_with_input(db).file_id
            } else {
                source.file_id.original_file(db)
            }.file_id(db);
            if let Some((_, avi)) = ast_collect.get_function_info(file_id, &ast_collect::get_ast_fn_key(&source.value)) {
                if visit.is_visited() {
                    *avi = ast_collect::AstVisitInfo::Visited;
                } else if *avi == ast_collect::AstVisitInfo::None { // Do not make it unused if it is already visited in HIR-level.
                    *avi = ast_collect::AstVisitInfo::Unused; // It is recognized in HIR-level and unused.
                }
            }
        }
    }
}

/// Kind of the unused item.
#[derive(Debug)]
enum UnusedItemKind {
    /// It is a function. 'fn'
    Function,
    /// It is a use statement. 'use'
    Use,
}

/// Represents Unused Items
#[derive(Debug)]
struct UnusedItem {
    /// The function key
    key: String,
    /// The range of the function in the source code
    text_range: TextRange,
    /// The kind of the unused item
    kind: UnusedItemKind
}

impl UnusedItem {
    fn new(key: String, text_range: TextRange, kind: UnusedItemKind) -> Self {
        Self {
            key,
            text_range,
            kind
        }
    }
}

impl PartialEq for UnusedItem {
    fn eq(&self, other: &Self) -> bool {
        self.text_range.start() == other.text_range.start()
    }
}
impl Eq for UnusedItem {}
impl PartialOrd for UnusedItem {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.text_range.start().partial_cmp(&other.text_range.start())
    }
}
impl Ord for UnusedItem {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.text_range.start().cmp(&other.text_range.start())
    }
}

/// Represents the result of unused analysis.
pub struct UnusedAnalysisResult {
    /// Ranges to be disabled.
    unuseds : HashMap<PathBuf, Vec<UnusedItem>>
}

impl UnusedAnalysisResult {
    /// Get the content whose unused items are disabled.
    /// Items are disabled with `#[cfg(not(feature="cfg_feature"))]` attribute.
    pub fn get_disabled_content<P: AsRef<Path>, S: AsRef<str>>(&self, path: P, cfg_feature: S) -> Option<String> {
        let items = self.unuseds.get(path.as_ref())?;
        let src = std::fs::read_to_string(path.as_ref()).ok()?;
        Some(Self::impl_disabled_content(&src, items, cfg_feature.as_ref()))
    }

    /// Get the content by using the analysis cache file.
    /// If the cache file does not exist or is invalid, returns `None`.
    pub fn get_disabled_content_by_file<P1: AsRef<Path>, P2: AsRef<Path>, S: AsRef<str>>(edition: ra_ap_syntax::Edition, path: P1, cache_path: P2, cfg_feature: S, features: &Vec<String>) -> Option<String> {
        let cache = std::fs::read_to_string(cache_path.as_ref()).ok()?;
        let unuseds : HashSet<_> = cache.split('\n').filter(|p| !p.trim().is_empty()).collect();

        let src = std::fs::read_to_string(path.as_ref()).ok()?;
        let parse = ra_ap_syntax::SourceFile::parse(&src, edition);
        let file = parse.tree();
        let mut collect = ast_collect::AstCollectFile::new_from_file(&file, use_elimination::from_cache_file(&file, &unuseds), features);
        for (key, (_, avi)) in collect.functions.iter_mut() {
            if *avi == ast_collect::AstVisitInfo::None && unuseds.contains(key.as_str()) {
                *avi = ast_collect::AstVisitInfo::Unused;
            }
        }
        Some(Self::impl_disabled_content(&src, &collect.into_vec_unuseditem(), cfg_feature.as_ref()))
    }

    /// The disabled content is generated by adding `#[cfg(not(feature="cfg_feature"))]` attribute to the unused items.
    fn impl_disabled_content(src_text: &str, unused_items: &[UnusedItem], cfg_feature: &str) -> String {
        let cfg_str = format!("#[cfg(not(feature=\"{}\"))]\n", cfg_feature);
        let mut result = String::with_capacity(src_text.len() + unused_items.len() * cfg_str.len());
        let mut itr = 0;
        for UnusedItem { text_range: r, kind: k, .. } in unused_items {
            result.push_str(&src_text[itr..r.start().into()]);
            match k {
                UnusedItemKind::Function => {
                    result.push_str(&cfg_str);
                    result.push_str(&src_text[*r]);
                }
                UnusedItemKind::Use => {
                    result.push_str("/*");
                    result.push_str(&src_text[*r]);
                    result.push_str("*/");
                }
            }
            itr = r.end().into();
        }
        if itr < src_text.len() {
            result.push_str(&src_text[itr..]);
        }
        result
    }

    /// Generate analysis cache file content.
    /// If there are no unused items (i.e., all items in the file are used by somewhat), returns `None`.
    pub fn get_analysis_cache<P: AsRef<Path>>(&self, path: P) -> Option<String> {
        let items = self.unuseds.get(path.as_ref())?;
        let mut result = String::new();
        for item in items.iter() {
            result.push_str(item.key.as_ref());
            result.push('\n');
        }
        Some(result)
    }
}

// struct CallGraph {
//     nodes: std::collections::HashSet<ra_ap_hir::Function>,
//     edges: std::collections::HashMap<ra_ap_hir::Function, std::collections::HashSet<ra_ap_hir::Function>>,
// }
// impl CallGraph {
//     fn add_edge(&mut self, caller: ra_ap_hir::Function, callee: ra_ap_hir::Function) {
//         self.nodes.insert(caller);
//         self.nodes.insert(callee);
//         self.edges.entry(caller).or_insert_with(std::collections::HashSet::new).insert(callee);
//     }
// }
// fn print_all_traits(db: &RootDatabase, def: ra_ap_hir::ModuleDef) {
//     match &def {
//         ra_ap_hir::ModuleDef::Trait(t) => println!("Trait: {} id:{:?}", t.name(db).display(db, ra_ap_ide::Edition::Edition2024), t),
//         _ => {}
//     }
// }

// fn print_all_dependencies(callgraph: &mut CallGraph, db: &RootDatabase, def : ra_ap_hir::ModuleDef) {
//     let sema = Semantics::new(db);
//     match &def {
//         ra_ap_hir::ModuleDef::Function(f) => {
//             let name = f.name(db);
//             println!("Function: {} id:{:?}", name.display(db, ra_ap_ide::Edition::Edition2024), f);
//             for ty in DefWithBody::Function(*f).expression_types(db) {  
//                 println!("  related types: {:?}", ty);
//             }  
//             if let Some(s) = sema.source(*f) && let Some(b) = s.value.body() {
//                 for expr in b.syntax().descendants().filter_map(ast::CallableExpr::cast) {
//                     match expr {
//                         ast::CallableExpr::Call(call) => {
//                             if let Some(expr) = call.expr() {
//                                 if let Some(ty) = sema.type_of_expr(&expr) {
//                                     if let Some(callable) = ty.adjusted().as_callable(db) {
//                                         // Local function call.
//                                         if let ra_ap_hir::CallableKind::Function(resolved) = callable.kind() {  
//                                             if let Some(ai) = resolved.as_assoc_item(sema.db) {
//                                                 match ai.container(sema.db) {
//                                                     ra_ap_hir::AssocItemContainer::Impl(impl_) => {
//                                                         let ty = impl_.self_ty(sema.db);
//                                                         println!("  {:?} -C> {} id:{:?} in impl {}", expr.syntax().text(), resolved.name(db).display(db, ra_ap_ide::Edition::Edition2024), resolved, ty.display(db, ra_ap_hir::DisplayTarget::from_crate(db, ty.krate(db).into())));
//                                                     }
//                                                     ra_ap_hir::AssocItemContainer::Trait(trait_) => {
//                                                         println!("  {:?} -C> {} id:{:?} in trait {}", expr.syntax().text(), resolved.name(db).display(db, ra_ap_ide::Edition::Edition2024), resolved, trait_.name(db).display(db, ra_ap_ide::Edition::Edition2024));
//                                                     }
//                                                 }
//                                             }
//                                             else 
//                                             {
//                                                 // func is the callee  
//                                                 println!("  {:?} -C> {} id:{:?} in module {:?}", expr.syntax().text(), resolved.name(db).display(db, ra_ap_ide::Edition::Edition2024), resolved, resolved.module(db).name(db));
//                                             }
//                                             callgraph.add_edge(f.clone(), resolved.clone());
//                                         }   else {
//                                             println!("  {:?} -C> Something else", expr.syntax().text());
//                                         }
//                                     } else  {
//                                         println!(" NOT CALLABLE {}", expr.syntax().text());
//                                     }
//                                 } else {
//                                     println!("  -C> <unresolved> {}", expr.syntax().text());
//                                 }
//                             }
//                         }
//                         ast::CallableExpr::MethodCall(call) => {  
//                             if let Some(resolved) = ra_ap_hir::attach_db(db, || {sema.resolve_method_call(&call)}) {
//                                 println!("  {:?} -MC> {} id:{:?} in module {:?}", call.syntax().text(), resolved.name(db).display(db, ra_ap_ide::Edition::Edition2024), resolved, resolved.module(db).name(db));
//                                 callgraph.add_edge(f.clone(), resolved.clone());
//                             } else {
//                                 println!("  {:?} -MC> <unresolved> {}", call.syntax().text(), call.syntax().text());
//                             }
//                             // func is the callee  
//                         }
//                     }
//                 }
//             }
//         }
//         ra_ap_hir::ModuleDef::Module(m) => {
//             for sub_def in m.declarations(db) {
//                 print_all_dependencies(callgraph, db, sub_def);
//             }
//         }
//         _ => {}
//     }
// }
