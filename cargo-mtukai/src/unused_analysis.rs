use std::{collections::HashMap, fmt::Debug, path::{Path, PathBuf}};

use anyhow::{Context, Result};
use ra_ap_ide::{RootDatabase, Semantics, TextRange};
use ra_ap_load_cargo::{load_workspace_at, LoadCargoConfig, ProcMacroServerChoice};
use ra_ap_project_model::{CargoConfig, CargoFeatures};
use ra_ap_vfs::{VfsPath, Vfs};
use ra_ap_base_db::{SourceDatabase};
use ra_ap_hir::{AsAssocItem, Crate};
use ra_ap_syntax::ast::{self, AstNode, HasAttrs, HasModuleItem, HasName};

// fn textrange_to_line_column<S: AsRef<str>, T: AsRef<str>>(path_str: S, txt : T, range: ra_ap_ide::TextRange) -> String {
//     let start_line_col = txt.as_ref()[..<usize>::from(range.start())].lines().count();
//     let end_line_col = txt.as_ref()[..range.end().into()].lines().count();
//     // format!("{} {}:{:?}-{}:{:?}", path_str.as_ref(), start_line_col + 1, range.start(), end_line_col + 1, range.end())
//     format!("{}:{}-{}", path_str.as_ref(), start_line_col + 1, end_line_col + 1)
// }

pub fn analyze_unused<S: AsRef<str>, S1: AsRef<str>>(manifest_path: &Path, features: S, entry_point_name : Option<S1>) -> Result<UnusedAnalysisResult> {
    let manifest_file = if manifest_path.is_file() {
        manifest_path.to_path_buf()
    } else {
        manifest_path.join("Cargo.toml")
    };
    let workspace_root = manifest_file
        .parent()
        .context("manifest path has no parent directory")?
        .canonicalize()
        .context("failed to canonicalize workspace root")?;

    let feature_list: Vec<String> = features.as_ref()
        .split(',')
        .map(str::trim)
        .filter(|feature| !feature.is_empty())
        .map(ToOwned::to_owned)
        .collect();

    let cargo_config = CargoConfig {
        sysroot: Some(ra_ap_project_model::RustLibSource::Discover),
        all_targets: true,
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
        manifest_file.parent().ok_or_else(|| anyhow::anyhow!("manifest path has no parent directory"))?.canonicalize()?.to_str().ok_or_else(|| anyhow::anyhow!("failed to convert workspace root to string"))?.to_owned());
    println!("Workspace root: {}", workspace_root);
    let bin_root = workspace_root.join("src").unwrap().join("bin").unwrap().join("main.rs").unwrap();
    let bin_root_id = vfs.file_id(&bin_root).unwrap().0;
    
    let root_krate = Crate::all(&root_db).into_iter().find(|c| c.root_file(&root_db) == bin_root_id).unwrap();
      
    let root_file = root_krate.root_file(&root_db);  
    let source_root_id = root_db.file_source_root(root_file).source_root_id(&root_db);  
    let source_root = root_db.source_root(source_root_id).source_root(&root_db);  

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

#[derive(Debug)]
struct VisitInfo {
    visited : bool,
    force_visited : bool,
}

impl VisitInfo {
    fn force_visited() -> Self {
        Self {
            visited : false,
            force_visited : true
        }
    }
    fn new(force_visited : bool) -> Self {
        Self {
            visited : false,
            force_visited
        }
    }

    fn visited(&mut self) {
        self.visited = true;
    }
    fn is_visited(&self) -> bool {
        self.visited
    }
}

#[derive(Debug)]
enum AstVisitInfo {
    None,
    Unused,
    Visited
}

pub struct UnusedAnalysisResult {
    /// Ranges to be disabled.
    disable_range : HashMap<PathBuf, Vec<TextRange>>
}

impl UnusedAnalysisResult {
    pub fn get_disabled_content<P: AsRef<Path>>(&self, path: P) -> Option<String> {
        let ranges = self.disable_range.get(path.as_ref())?;
        let src = std::fs::read_to_string(path.as_ref()).ok()?;
        let mut result = String::with_capacity(src.len() + 8*ranges.len());
        let mut itr = 0;
        for r in ranges.iter() {
            result.push_str(&src[itr..r.start().into()]);
            result.push_str("/*\n");
            result.push_str(&src[*r]);
            result.push_str("\n*/\n");
            itr = r.end().into();
        }
        if itr < src.len() {
            result.push_str(&src[itr..]);
        }
        Some(result)
    }
}

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
                thispart.push_str("for");
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

#[derive(Debug)]
struct AstCollectFile {
    functions: HashMap<String, (ra_ap_syntax::ast::Fn, AstVisitInfo)>
}

#[derive(Debug)]
struct AstCollect {
    files : HashMap<ra_ap_vfs::FileId, AstCollectFile>,
}

impl AstCollect {
    fn new() -> Self {
        Self {
            files: HashMap::new()
        }
    }
    fn insert_function(&mut self, file_id: ra_ap_vfs::FileId, func: ra_ap_syntax::ast::Fn, visit: AstVisitInfo) {
        let key = get_ast_fn_key(&func);
        let file_entry = self.files.entry(file_id).or_insert_with(|| AstCollectFile { functions: HashMap::new() });
        file_entry.functions.insert(key, (func, visit));
    }
    // fn get_visit_info(&mut self, file_id: ra_ap_vfs::FileId, key: &str) -> Option<&mut AstVisitInfo> {
    //     self.get_function_info.and_then(|&mut (func, visit)| Some(visit))
    // }
    fn get_function_info<S: AsRef<str>>(&mut self, file_id: ra_ap_vfs::FileId, key: S) -> Option<&mut (ra_ap_syntax::ast::Fn, AstVisitInfo)> {
        self.files.get_mut(&file_id).and_then(|file_entry| file_entry.functions.get_mut(key.as_ref()))
    }
    fn collect_from_file(&mut self, file_id : &ra_ap_vfs::FileId, db : &RootDatabase, root_krate : &Crate) {
        let text = db.file_text(file_id.clone()).text(db);
        let parse = ra_ap_syntax::SourceFile::parse(&text, root_krate.edition(db));
        let file = parse.tree();

        fn has_panic_handler(func: &ast::Fn) -> bool {
            func.attrs().any(|attr| {
                attr.path().map(|p| p.syntax().text().to_string() == "panic_handler").unwrap_or(false)
            })
        }
            
        for item in file.items() {
            match item {
                ast::Item::Fn(func) => {
                    let visit = if has_panic_handler(&func) { AstVisitInfo::Visited } else { AstVisitInfo::None };
                    self.insert_function(file_id.clone(), func.clone(), visit);
                }
                ast::Item::Impl(impl_) => {
                    // functions inside impl block
                    if let Some(assoc_item_list) = impl_.assoc_item_list() {
                        for assoc_item in assoc_item_list.assoc_items() {
                            if let ast::AssocItem::Fn(func) = assoc_item {
                                let visit = if has_panic_handler(&func) { AstVisitInfo::Visited } else { AstVisitInfo::None };
                                self.insert_function(file_id.clone(), func.clone(), visit);
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
                                    self.insert_function(file_id.clone(), func.clone(), visit);
                                }
                            }
                        }
                    }
                }
                _ => {}  
            }  
        }  
    }

    fn into_unused_analysis_result(self, vfs: &Vfs) -> UnusedAnalysisResult {
        let mut disable_range  = HashMap::new();
        for (path, file_entry) in self.files.into_iter()
            .filter_map(|(file_id, file_entry)|
                vfs.file_path(file_id).as_path().map(|v| v.to_path_buf()).and_then(|p| Some((p, file_entry)))
            ) {
            let mut ranges: Vec<TextRange> = file_entry.functions.into_iter()
                .filter_map(|(_, (func, visit))| {
                    if let AstVisitInfo::Unused = visit {
                        Some(func.syntax().text_range())
                    } else {
                        None
                    }
                })
                .collect();
            if !ranges.is_empty() {
                ranges.sort_by(|a, b| a.start().cmp(&b.start()));
                disable_range.insert(path.into(), ranges);
            }
        }
        UnusedAnalysisResult { disable_range }
    }
}


#[derive(Debug)]
struct HirCollect {
    functions: HashMap<ra_ap_hir::Function, VisitInfo>,
    traits: HashMap<ra_ap_hir::Trait, VisitInfo>,
    adts: HashMap<ra_ap_hir::Adt, VisitInfo>,
    consts: HashMap<ra_ap_hir::Const, VisitInfo>,
    statics: HashMap<ra_ap_hir::Static, VisitInfo>,
}

impl HirCollect {
    fn new<S: AsRef<str>>(root_krate: &Crate, db: &RootDatabase, entrypoint_name : Option<S>) -> Option<Self> {
        let mut functions = HashMap::new();
        let mut traits = HashMap::new();
        let mut adts = HashMap::new();
        let mut consts = HashMap::new();
        let mut statics = HashMap::new();
        let sema = Semantics::new(db);
        let mut found_main = false;

        fn do_module<S: AsRef<str>>(m: ra_ap_hir::Module, functions: &mut HashMap<ra_ap_hir::Function, VisitInfo>,
            traits: &mut HashMap<ra_ap_hir::Trait, VisitInfo>, adts: &mut HashMap<ra_ap_hir::Adt, VisitInfo>,
            consts: &mut HashMap<ra_ap_hir::Const, VisitInfo>, statics: &mut HashMap<ra_ap_hir::Static, VisitInfo>,
            found_main: &mut bool, sema: &Semantics<RootDatabase>, db: &RootDatabase, entrypoint_name : &Option<S>, entry_str: &ra_ap_syntax::SmolStr) {
            for imp in m.impl_defs(db) {
                let force_visited = imp.trait_(db).is_some();
                for i in imp.items(db).iter() {
                    match i {
                        ra_ap_hir::AssocItem::Function(f) => {
                            functions.insert(f.clone(), VisitInfo::new(force_visited));
                        }
                        ra_ap_hir::AssocItem::Const(c) => {
                            consts.insert(c.clone(), VisitInfo::new(force_visited));
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
                        do_module(m, functions, traits, adts, consts, statics, found_main, sema, db, entrypoint_name, entry_str);
                    }
                    ra_ap_hir::ModuleDef::Function(f) => {
                        let is_visited = 
                        if let Some(entry_name) = entrypoint_name {
                            f.name(db).as_str() == entry_name.as_ref()
                        } else {
                            f.is_main(db)
                        };
                        *found_main |= is_visited;
                        functions.insert(f, VisitInfo::new(is_visited));
                    }
                    ra_ap_hir::ModuleDef::Adt(adt) => {
                        adts.insert(adt, VisitInfo::force_visited());
                    }
                    ra_ap_hir::ModuleDef::Const(c) => {
                        consts.insert(c, VisitInfo::force_visited());
                    }
                    ra_ap_hir::ModuleDef::Static(st) => {
                        statics.insert(st, VisitInfo::force_visited());
                    }
                    ra_ap_hir::ModuleDef::Trait(t) => {
                        traits.insert(t, VisitInfo::force_visited());
                        for item in t.items(db) {
                            if let ra_ap_hir::AssocItem::Function(f) = item {
                                if let Some(source) = sema.source(f.clone()) {
                                    if source.value.body().is_some() {
                                        functions.insert(f, VisitInfo::new(true)); // CURRENTLY
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
            do_module(d, &mut functions, &mut traits, &mut adts, &mut consts, &mut statics, &mut found_main, &sema, db, &entrypoint_name, &ra_ap_syntax::SmolStr::new("entry"));
        }
        if found_main {
            Some(Self {
                functions,
                traits,
                adts,
                consts,
                statics
            })
        } else { None }
    }
}

fn check_used_functions_in_ast(hircollect: &HirCollect, ast_collect: &mut AstCollect, db: &RootDatabase) {
    let sema = Semantics::new(db);
    for (f, visit) in hircollect.functions.iter() {
        if let Some(source) = sema.source(f.clone()) {
            let file_id = if let Some(mf) = source.file_id.macro_file() {
                mf.loc(db).kind.original_call_range_with_input(db).file_id.file_id(db)
            } else {
                source.file_id.original_file(db).file_id(db)
            };
            if let Some(vis) = ast_collect.get_function_info(file_id, &get_ast_fn_key(&source.value)) {
                // println!("hir function {} -> ast function {} is {:?}", f.name(db).display(db, ra_ap_ide::Edition::Edition2024), vis.0.name().map(|n| n.text().to_string()).unwrap_or_else(|| "<unnamed>".to_string()), if visit.is_visited() { "visited" } else { "unused" });
                if visit.is_visited() {
                    vis.1 = AstVisitInfo::Visited;
                } else if matches!(vis.1, AstVisitInfo::None) {
                    vis.1 = AstVisitInfo::Unused;
                }
            }
        }
    }
}

fn walk_dependency(collect: &mut HirCollect, db: &RootDatabase) {
    let mut walkqueue = Vec::<ra_ap_hir::Function>::new();
    for (f, val) in collect.functions.iter_mut().filter(|(_, v)| v.force_visited){
        walkqueue.push(f.clone());
        val.visited();
    }
    let sema = Semantics::new(db);
    fn queue_function(db: &RootDatabase, f: ra_ap_hir::Function, collect: &mut HirCollect, walkqueue: &mut Vec<ra_ap_hir::Function>) {
        fn require_visit(f: &ra_ap_hir::Function, collect: &mut HirCollect) -> bool {
            if let Some(is_visited) = collect.functions.get_mut(f) {
                let ret = !is_visited.is_visited();
                is_visited.visited();
                ret
            } else {
                false
            }
        }
        fn visit_all_trait_implementers(db: &RootDatabase, function_name: &ra_ap_hir::Name, trait_: &ra_ap_hir::Trait, collect: &mut HirCollect, walkqueue: &mut Vec<ra_ap_hir::Function>) {
            for imp in ra_ap_hir::Impl::all_for_trait(db, *trait_).into_iter() {
                for item in imp.items(db).iter() {
                    if let ra_ap_hir::AssocItem::Function(func) = item
                    && &func.name(db) == function_name {
                        if require_visit(&func, collect) {
                            walkqueue.push(func.clone());
                        }
                    }
                }
            }
        }
        if require_visit(&f, collect) {
            if let Some(assoc) = f.as_assoc_item(db) {  
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
    while let Some(f) = walkqueue.pop() {
        if let Some(b) = sema.source(f) {
            let syntax = b.value.syntax();
            // Function Calling
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
                                queue_function(db, resolved.clone(), collect, &mut walkqueue);
                            }
                        }
                    }
                    ast::CallableExpr::MethodCall(call) => {
                        if let Some(resolved) = sema.resolve_method_call(&call) {
                            queue_function(db, resolved.clone(), collect, &mut walkqueue);
                        } else {
                        }
                    }
                }
            }
            // Higher-order function
            for pr in syntax.descendants()
                .filter_map(ast::PathExpr::cast)
                .filter_map(|path_expr| path_expr.path().and_then(|path| sema.resolve_path(&path))) {
                if let ra_ap_hir::PathResolution::Def(ra_ap_hir::ModuleDef::Function(resolved)) = pr {
                    queue_function(db, resolved, collect, &mut walkqueue);
                }
            }
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