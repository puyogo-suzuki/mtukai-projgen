use std::{cell::Cell, collections::{HashMap, HashSet}, fmt::Debug, path::{Path, PathBuf}};

use anyhow::{Context, Result};
use ra_ap_ide::{RootDatabase, Semantics, TextRange};
use ra_ap_load_cargo::{load_workspace_at, LoadCargoConfig, ProcMacroServerChoice};
use ra_ap_project_model::{CargoConfig, CargoFeatures};
use ra_ap_vfs::{VfsPath, Vfs};
use ra_ap_base_db::{SourceDatabase};
use ra_ap_hir::{AsAssocItem, Crate};
use ra_ap_syntax::ast::{self, AstNode, HasAttrs, HasModuleItem, HasName};

mod use_elimination;

// fn textrange_to_line_column<S: AsRef<str>, T: AsRef<str>>(path_str: S, txt : T, range: ra_ap_ide::TextRange) -> String {
//     let start_line_col = txt.as_ref()[..<usize>::from(range.start())].lines().count();
//     let end_line_col = txt.as_ref()[..range.end().into()].lines().count();
//     // format!("{} {}:{:?}-{}:{:?}", path_str.as_ref(), start_line_col + 1, range.start(), end_line_col + 1, range.end())
//     format!("{}:{}-{}", path_str.as_ref(), start_line_col + 1, end_line_col + 1)
// }

/// Aanalyze to detect the unused items.
/// `features` must be comma-separated list of features to enable. If empty, all features are enabled.
/// `entry_point_name` is the name of the entry point function. If None, the main function is used as the entry point.
pub fn analyze_unused<S1: AsRef<str> + Debug, S2: AsRef<str>, S3: AsRef<str>>(manifest_path: &Path, target: &Option<S1>, features: S2, entry_point_name : Option<S3>) -> Result<UnusedAnalysisResult> {
    let root_crate_root = if manifest_path.is_file() {
        manifest_path.parent().unwrap_or(manifest_path)
    } else {
        manifest_path
    };
    let workspace_root = root_crate_root
        .canonicalize()
        .context("failed to canonicalize workspace root")?;

    let feature_list: Vec<String> = features.as_ref()
        .split(',')
        .map(str::trim)
        .filter(|feature| !feature.is_empty())
        .map(ToOwned::to_owned)
        .collect();
    let all_targets = target.is_none();
    let cargo_config = CargoConfig {
        sysroot: Some(ra_ap_project_model::RustLibSource::Discover),
        target: target.as_ref().map(|t| t.as_ref().to_string()),
        all_targets,
        features : if feature_list.is_empty() {
            CargoFeatures::All
        } else {
            CargoFeatures::Selected {
                features: feature_list,
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
        let mut collections = HirCollect::new(&root_krate, &root_db, entry_point_name)?;
        // println!("Result: {:?}", collections);
        walk_dependency(&mut collections, &root_db);
        Some(collections)
    }).context("Failed to search the main function")?;
    let mut ast_collect = AstCollect::new();
    for file_id in source_root.iter() {
        if let Some(path) = source_root.path_for_file(&file_id) { 
            // exclude template and generated directory.
            if (gen_dir.as_ref().map(|d| !path.starts_with(&d)).unwrap_or(false) && tem_dir.as_ref().map(|d| !path.starts_with(&d)).unwrap_or(false))
            && let Some((_, Some("rs"))) = path.name_and_extension() {  
                ast_collect.collect_from_file(&file_id, &root_db, &root_krate);
            }
        }
    }

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
    //     }
    //     else {
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

/// Represents Unused Items
struct UnusedItem {
    /// The function key
    function_key : String,
    /// The range of the function in the source code
    text_range : TextRange
}

impl UnusedItem {
    fn new(function_key: String, text_range: TextRange) -> Self {
        Self {
            function_key,
            text_range
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

    /// The disabled content is generated by adding `#[cfg(not(feature="cfg_feature"))]` attribute to the unused items.
    fn impl_disabled_content(src_text: &str, unused_items: &Vec<UnusedItem>, cfg_feature: &str) -> String {
        let cfg_str = format!("#[cfg(not(feature=\"{}\"))]\n", cfg_feature);
        let mut result = String::with_capacity(src_text.len() + unused_items.len() * cfg_str.len());
        let mut itr = 0;
        for r in unused_items.iter().map(|item| item.text_range) {
            result.push_str(&src_text[itr..r.start().into()]);
            result.push_str(&cfg_str);
            result.push_str(&src_text[r]);
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
            result.push_str(item.function_key.as_ref());
            result.push('\n');
        }
        Some(result)
    }

    /// Get the content by using the analysis cache file.
    /// If the cache file does not exist or is invalid, returns `None`.
    pub fn get_disabled_content_by_file<P1: AsRef<Path>, P2: AsRef<Path>, S: AsRef<str>>(edition: ra_ap_syntax::Edition, path: P1, cache_path: P2, cfg_feature: S) -> Option<String> {
        let cache = std::fs::read_to_string(cache_path.as_ref()).ok()?;
        let unused_functions = cache.split('\n').collect::<HashSet<_>>();

        let src = std::fs::read_to_string(path.as_ref()).ok()?;
        let parse = ra_ap_syntax::SourceFile::parse(&src, edition);
        let file = parse.tree();
        let mut collect = AstCollectFile::new_from_file(file);
        for (key, val) in collect.functions.iter_mut() {
            unused_functions.contains(key.as_str()).then(|| val.1 = AstVisitInfo::Unused);
        }
        Some(Self::impl_disabled_content(&src, &collect.into_vec_unuseditem(), cfg_feature.as_ref()))
    }
}

/// Generate the identifier (key) for functions. e.g., impl:Foo::bar
fn get_ast_fn_key(func: &ast::Fn) -> String {
    let fn_name = func.name().map(|n| n.text().to_string()).unwrap_or_else(|| "<unnamed>".to_string());
    let mut parts = Vec::new();
    
    let mut current = func.syntax().parent();
    while let Some(node) = current {
        if let Some(impl_) = ast::Impl::cast(node.clone()) {
            let mut thispart = "impl:".to_string();
            let self_ty = impl_.self_ty().map(|t| t.syntax().text().to_string().replace([' ', '\n', '\r', '\t'], "")).unwrap_or_default();
            if let Some(tr) = impl_.trait_().map(|t| t.syntax().text().to_string().replace([' ', '\n', '\r', '\t'], "")) {
                thispart.push_str(&tr);
                thispart.push_str(":for:");
            }
            thispart.push_str(&self_ty);
            parts.push(thispart);
        } else if let Some(tr) = ast::Trait::cast(node.clone()) {
            parts.push(format!("trait:{}", tr.name().map(|n| n.text().to_string()).unwrap_or_default()));
        } else if let Some(m) = ast::Module::cast(node.clone()) {
            parts.push(format!("mod:{}", m.name().map(|n| n.text().to_string()).unwrap_or_default()));
        }
        current = node.parent();
    }
    
    parts.reverse();
    parts.push(format!("fn:{}", fn_name));
    parts.join("::")
}

/// Collected File Information in AST-level
#[derive(Debug)]
struct AstCollectFile {
    functions: HashMap<String, (ra_ap_syntax::ast::Fn, AstVisitInfo)>
}
impl AstCollectFile {
    /// Returns a new `AstCollectFile` instance by collecting function information from the given `file`.
    fn new_from_file(file: ra_ap_syntax::SourceFile) -> AstCollectFile {
        // Check if the function has a `#[panic_handler]` attribute.
        // panic_handlers are used.
        fn has_panic_handler(func: &ast::Fn) -> bool {
            func.attrs().any(|attr| {
                attr.path().map(|p| p.syntax().text().to_string() == "panic_handler").unwrap_or(false)
            })
        }

        let mut functions = HashMap::new();
        for item in file.items() {
            match item {
                ast::Item::Fn(func) => {
                    let visit = if has_panic_handler(&func) { AstVisitInfo::Visited } else { AstVisitInfo::None };
                    functions.insert(get_ast_fn_key(&func), (func, visit));
                }
                ast::Item::Impl(impl_) => {
                    // functions inside impl block
                    if let Some(assoc_item_list) = impl_.assoc_item_list() {
                        for assoc_item in assoc_item_list.assoc_items() {
                            if let ast::AssocItem::Fn(func) = assoc_item {
                                let visit = if has_panic_handler(&func) { AstVisitInfo::Visited } else { AstVisitInfo::None };
                                functions.insert(get_ast_fn_key(&func), (func, visit));
                            }
                        }
                    }
                }
                ast::Item::Trait(trait_) => {  
                    // functions inside trait definition  
                    if let Some(assoc_item_list) = trait_.assoc_item_list() {  
                        for assoc_item in assoc_item_list.assoc_items() {  
                            if let ast::AssocItem::Fn(func) = assoc_item {
                                if func.body().is_some() {
                                    let visit = if has_panic_handler(&func) { AstVisitInfo::Visited } else { AstVisitInfo::None };
                                    functions.insert(get_ast_fn_key(&func), (func, visit));
                                }
                            }
                        }
                    }
                }
                _ => {}  
            }  
        }  
        AstCollectFile { functions }
    }

    /// Converts `functions` into the `Vec` of `UnusedItem`s.
    fn into_vec_unuseditem(&self) -> Vec<UnusedItem> {
        let mut ranges: Vec<UnusedItem> = self.functions.values().filter_map(|(func, visit)| {
            if let AstVisitInfo::Unused = visit {
                Some(UnusedItem::new(get_ast_fn_key(&func), func.syntax().text_range()))
            } else {
                None
            }
        }).collect();
        ranges.sort();
        ranges
    }
}

/// Visit Information for `AstCollect`
#[derive(Debug, Eq, PartialEq)]
enum AstVisitInfo {
    /// It is not recognized in HIR-level.
    None,
    /// It is unused.
    Unused,
    /// It is used.
    Visited
}

/// Collected Information in AST-level
#[derive(Debug)]
struct AstCollect {
    files : HashMap<ra_ap_vfs::FileId, AstCollectFile>,
}

impl AstCollect {
    fn new() -> Self {
        Self { files: HashMap::new() }
    }
    fn get_function_info<S: AsRef<str>>(&mut self, file_id: ra_ap_vfs::FileId, key: S) -> Option<&mut (ra_ap_syntax::ast::Fn, AstVisitInfo)> {
        self.files.get_mut(&file_id).and_then(|file_entry| file_entry.functions.get_mut(key.as_ref()))
    }
    fn collect_from_file(&mut self, file_id : &ra_ap_vfs::FileId, db : &RootDatabase, root_krate : &Crate) {
        let text = db.file_text(file_id.clone()).text(db);
        let parse = ra_ap_syntax::SourceFile::parse(&text, root_krate.edition(db));
        let file = parse.tree();

        self.files.insert(file_id.clone(), AstCollectFile::new_from_file(file));
    }

    fn into_unused_analysis_result(self, vfs: &Vfs) -> UnusedAnalysisResult {
        let mut res  = HashMap::new();
        for (path, file_entry) in self.files.into_iter()
            .filter_map(|(file_id, file_entry)|
                vfs.file_path(file_id).as_path().map(|v| v.to_path_buf()).and_then(|p| Some((p, file_entry)))
            ) {
            let ranges = file_entry.into_vec_unuseditem();
            if !ranges.is_empty() {
                res.insert(path.into(), ranges);
            }
        }
        UnusedAnalysisResult { unuseds: res }
    }
}



/// Visit Infomation for HIR
#[derive(Debug, PartialEq, Eq)]
struct HirVisit {
    visited : Cell<bool>
}

impl HirVisit {
    /// Create a new `HirVisit` instance with the `visited` flag set to `true`.
    fn force_visited() -> Self {
        Self::new(true)
    }
    /// Create a new `HirVisit` instance
    fn new(visited : bool) -> Self {
        Self {
            visited : Cell::new(visited),
        }
    }

    /// Set the `visited` flag to `true` and return the previous value of the `visited` flag.
    fn visit(&self) -> bool {
        let ret = self.is_visited();
        self.visited.set(true);
        ret
    }

    /// Check the whether the item is visited or force_visited.
    fn is_visited(&self) -> bool {
        self.visited.get()
    }
}

#[derive(Debug)]
struct HirCollect {
    functions: HashMap<ra_ap_hir::Function, HirVisit>,
    traits: HashMap<ra_ap_hir::Trait, HirVisit>,
    adts: HashMap<ra_ap_hir::Adt, HirVisit>,
    consts: HashMap<ra_ap_hir::Const, HirVisit>,
    statics: HashMap<ra_ap_hir::Static, HirVisit>,
}

impl HirCollect {
    /// Construct 'HirCollect'. Return `None` if the entry point function is not found.
    fn new<S: AsRef<str>>(root_krate: &Crate, db: &RootDatabase, entrypoint_name : Option<S>) -> Option<Self> {
        let mut result = HirCollect {
            functions: HashMap::new(),
            traits: HashMap::new(),
            adts: HashMap::new(),
            consts: HashMap::new(),
            statics: HashMap::new(),
        };
        let sema = Semantics::new(db);
        let mut found_main = false;

        fn do_module<S: AsRef<str>>(m: ra_ap_hir::Module, collect: &mut HirCollect,
            found_main: &mut bool, sema: &Semantics<RootDatabase>, db: &RootDatabase, entrypoint_name : &Option<S>, entry_str: &ra_ap_syntax::SmolStr) {
            for imp in m.impl_defs(db) {
                let force_visited = imp.trait_(db).is_some();
                for i in imp.items(db).iter() {
                    match i {
                        ra_ap_hir::AssocItem::Function(f) => {
                            collect.functions.insert(f.clone(), HirVisit::new(force_visited));
                        }
                        ra_ap_hir::AssocItem::Const(c) => {
                            collect.consts.insert(c.clone(), HirVisit::new(force_visited));
                        }
                        ra_ap_hir::AssocItem::TypeAlias(_) => {
                            // DO NOTHING
                        }
                    }
                }
            }
            for d in m.declarations(db) {
                match d {
                        ra_ap_hir::ModuleDef::Module(m) => {
                            do_module(m, collect, found_main, sema, db, entrypoint_name, entry_str);
                    }
                    ra_ap_hir::ModuleDef::Function(f) => {
                        let is_visited = 
                        if let Some(entry_name) = entrypoint_name {
                            f.name(db).as_str() == entry_name.as_ref()
                        } else {
                            f.is_main(db)
                        };
                        *found_main |= is_visited;
                        collect.functions.insert(f, HirVisit::new(is_visited));
                    }
                    ra_ap_hir::ModuleDef::Adt(adt) => {
                        collect.adts.insert(adt, HirVisit::force_visited());
                    }
                    ra_ap_hir::ModuleDef::Const(c) => {
                        collect.consts.insert(c, HirVisit::force_visited());
                    }
                    ra_ap_hir::ModuleDef::Static(st) => {
                        collect.statics.insert(st, HirVisit::force_visited());
                    }
                    ra_ap_hir::ModuleDef::Trait(t) => {
                        collect.traits.insert(t, HirVisit::force_visited());
                        for item in t.items(db) {
                            if let ra_ap_hir::AssocItem::Function(f) = item {
                                if let Some(source) = sema.source(f.clone()) {
                                    if source.value.body().is_some() {
                                        collect.functions.insert(f, HirVisit::new(true)); // CURRENTLY
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        for d in root_krate.modules(db) {
            do_module(d, &mut result, &mut found_main, &sema, db, &entrypoint_name, &ra_ap_syntax::SmolStr::new("entry"));
        }
        if found_main {
            Some(result)
        } else { None }
    }
}

fn check_used_functions_in_ast(hircollect: &HirCollect, ast_collect: &mut AstCollect, db: &RootDatabase) {
    let sema = Semantics::new(db);
    for (f, visit) in hircollect.functions.iter() {
        if let Some(source) = sema.source(f.clone()) {
            let file_id = if let Some(mf) = source.file_id.macro_file() {
                mf.loc(db).kind.original_call_range_with_input(db).file_id
            } else {
                source.file_id.original_file(db)
            }.file_id(db);
            if let Some(vis) = ast_collect.get_function_info(file_id, &get_ast_fn_key(&source.value)) {
                // println!("hir function {} -> ast function {} is {:?}", f.name(db).display(db, ra_ap_ide::Edition::Edition2024), vis.0.name().map(|n| n.text().to_string()).unwrap_or_else(|| "<unnamed>".to_string()), if visit.is_visited() { "visited" } else { "unused" });
                if visit.is_visited() {
                    vis.1 = AstVisitInfo::Visited;
                } else if vis.1 == AstVisitInfo::None { // Do not make it unused if it is already visited in HIR-level.
                    vis.1 = AstVisitInfo::Unused; // It is recognized in HIR-level and unused.
                }
            }
        }
    }
}

/// Walk the dependency.
fn walk_dependency(collect: &HirCollect, db: &RootDatabase) {
    let mut walkqueue = Vec::<ra_ap_hir::Function>::new();
    for (f, val) in collect.functions.iter().filter(|(_, v)| v.is_visited()){
        walkqueue.push(f.clone());
        val.visit();
    }
    let sema = Semantics::new(db);
    // Queue to walk
    fn queue_function(db: &RootDatabase, f: ra_ap_hir::Function, collect: &HirCollect, walkqueue: &mut Vec<ra_ap_hir::Function>) {
        // Is it required to visit?
        fn require_visit(f: &ra_ap_hir::Function, collect: &HirCollect) -> bool {
            collect.functions.get(f).and_then(|is_visited| Some(!is_visited.visit())).unwrap_or(false)
            // unwrap_or(false): Is the function not collected? wonderful.
        }
        // Visit all trait implementers
        fn visit_all_trait_implementers(db: &RootDatabase, function_name: &ra_ap_hir::Name, trait_: &ra_ap_hir::Trait, collect: &HirCollect, walkqueue: &mut Vec<ra_ap_hir::Function>) {
            for imp in ra_ap_hir::Impl::all_for_trait(db, *trait_).into_iter() {
                for item in imp.items(db).iter() {
                    if let ra_ap_hir::AssocItem::Function(func) = item
                            && &func.name(db) == function_name && require_visit(&func, collect) {
                                // The function name is same and it is required to visit.
                        walkqueue.push(func.clone());
                    }
                }
            }
        }
        if require_visit(&f, collect) { // Check whether it must be visited.
            if let Some(assoc) = f.as_assoc_item(db) {   // If it is an associated function.
                match assoc.container(db) {  
                    ra_ap_hir::AssocItemContainer::Trait(t) => {  
                        // trait Foo
                        visit_all_trait_implementers(db, &f.name(db), &t, collect, walkqueue);
                    }  
                    ra_ap_hir::AssocItemContainer::Impl(i) => {  
                        if let Some(t) =  i.trait_(db)  {
                            // impl Foo for Bar
                            visit_all_trait_implementers(db, &f.name(db), &t, collect, walkqueue);
                        } else {
                            // impl Bar
                            // DO NOTHING
                        }
                    }
                }  
            } else {  
                // Non associated function.
                // DO NOTHING
            }
            walkqueue.push(f.clone());
        }
    }
    fn scan_syntax_for_functions(
        db: &RootDatabase,
        syntax: &ra_ap_syntax::SyntaxNode,
        collect: &HirCollect,
        walkqueue: &mut Vec<ra_ap_hir::Function>,
        sema: &Semantics<RootDatabase>,
    ) {
        for expr in syntax.descendants().filter_map(ast::CallableExpr::cast) {
            match expr {
                ast::CallableExpr::Call(call) => {
                    if let Some(callable) = call.expr()
                        .and_then(|expr| sema.type_of_expr(&expr)) // Failure of inferring means the called function is not defined in user's crate.
                        .and_then(|ty| ty.adjusted().as_callable(db)) { // The fialure means that it is not callable.
                        if let ra_ap_hir::CallableKind::Function(resolved) = callable.kind() {
                            // Check only if it is function.
                            // Because: 
                            //  - TupleStruct and TupleEnumVariant are compiler intrinsics.
                            //  - The callees of the Closure, FnPtr, FnImpl are collected on the defined place.
                            queue_function(db, resolved.clone(), collect, walkqueue);
                        }
                    }
                }
                ast::CallableExpr::MethodCall(call) => {
                    if let Some(resolved) = sema.resolve_method_call(&call) {
                        queue_function(db, resolved.clone(), collect, walkqueue);
                    }
                }
            }
        }
        // Higher-order function
        for pr in syntax.descendants()
            .filter_map(ast::PathExpr::cast)
            .filter_map(|path_expr| path_expr.path().and_then(|path| sema.resolve_path(&path))) {
            if let ra_ap_hir::PathResolution::Def(ra_ap_hir::ModuleDef::Function(resolved)) = pr {
                queue_function(db, resolved, collect, walkqueue);
            }
        }
    }

    // Scan initializers of consts and statics
    for c in collect.consts.keys() {
        if let Some(body) = sema.source(*c).and_then(|s| s.value.body()) {
            scan_syntax_for_functions(db, body.syntax(), collect, &mut walkqueue, &sema);
        }
    }
    for st in collect.statics.keys() {
        if let Some(body) = sema.source(*st).and_then(|s| s.value.body()) {
            scan_syntax_for_functions(db, body.syntax(), collect, &mut walkqueue, &sema);
        }
    }

    while let Some(f) = walkqueue.pop() {
        if let Some(b) = sema.source(f) {
            scan_syntax_for_functions(db, b.value.syntax(), collect, &mut walkqueue, &sema);
        }
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
//         ra_ap_hir::ModuleDef::Trait(t) => {
//             println!("Trait: {} id:{:?}", t.name(db).display(db, ra_ap_ide::Edition::Edition2024), t);
//         }
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
//             if let Some(s) = sema.source(*f)
//                 && let Some(b) = s.value.body() {
//                 for expr in b.syntax().descendants()
//                     .filter_map(ast::CallableExpr::cast) {
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
//                             let func = ra_ap_hir::attach_db(db, || {sema.resolve_method_call(&call)});
//                             if let Some(resolved) = func {
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