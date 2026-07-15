use std::{collections::{self, HashMap, HashSet}, fmt::Debug, fs, io::Write, path::Path};

use anyhow::{Context, Result};
use ra_ap_ide::{Edition::Edition2024, RootDatabase, Semantics};
use ra_ap_load_cargo::{load_workspace_at, LoadCargoConfig, ProcMacroServerChoice};
use ra_ap_project_model::{CargoConfig, CargoFeatures, ManifestPath, ProjectWorkspace};
use ra_ap_vfs::{VfsPath, Vfs};
use ra_ap_base_db::{SourceDatabase};
use ra_ap_hir::{AsAssocItem, Crate, DefWithBody, DisplayTarget, EditionedFileId, HasCrate, HirDisplay};
use ra_ap_syntax::{SyntaxNode, ast::{self, AstNode, HasName}};

pub fn analyze_def_use(manifest_path: &Path, features: String) -> Result<()> {
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

    let feature_list: Vec<String> = features
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
    // let excludes = vec![workspace_root.join("generated").unwrap(), workspace_root.join("template").unwrap()];
    
    let root_krate = Crate::all(&root_db).into_iter().find(|c| c.root_file(&root_db) == bin_root_id).unwrap();
    
    // let mut callgraph = CallGraph {
    //     nodes: std::collections::HashSet::new(),
    //     edges: std::collections::HashMap::new(),
    // };
    ra_ap_hir::attach_db(&root_db, || {
        let mut collections = Collect::new(&root_krate, &root_db, Some("__risc_v_rt__main")).unwrap();
        println!("Result: {:?}", collections);
        walk_dependency(&mut collections, &root_db);

        for (f, is_visit) in collections.functions.iter() {
            println!("Function {} (id: {:?}) is {}", f.name(&root_db).display(&root_db, ra_ap_ide::Edition::Edition2024), f, if is_visit.is_visited() { "visited" } else { "not visited" });
        }

        // for m in root_krate.modules(&root_db) {
        //     for d in m.declarations(&root_db) {
        //         match d {
        //             ra_ap_hir::ModuleDef::Function(f) => {
        //                 println!("Function: {} id:{:?}", f.name(&root_db).display(&root_db, ra_ap_ide::Edition::Edition2024), f);
        //             }
        //             ra_ap_hir::ModuleDef::Module(m) => {
        //                 println!("Module: {:?} id:{:?}", m.name(&root_db).and_then(  |n| Some(n.display(&root_db, ra_ap_ide::Edition::Edition2024).to_string())), m);
        //             }
        //             _ => {}
        //         }
        //         print_all_dependencies(&mut callgraph, &root_db, d);
        //     }
        // }
    });
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
    Ok(())
}

#[derive(Debug)]
struct VisitInfo {
    visited : bool,
    force_visited : bool,
}

impl VisitInfo {
    fn zero() -> Self {
        Self {
            visited : false,
            force_visited : false
        }
    }
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
struct Collect {
    functions: HashMap<ra_ap_hir::Function, VisitInfo>,
    traits: HashMap<ra_ap_hir::Trait, VisitInfo>,
    adts: HashMap<ra_ap_hir::Adt, VisitInfo>,
    consts: HashMap<ra_ap_hir::Const, VisitInfo>,
    statics: HashMap<ra_ap_hir::Static, VisitInfo>,
}

impl Collect {
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

struct CallGraph {
    nodes: std::collections::HashSet<ra_ap_hir::Function>,
    edges: std::collections::HashMap<ra_ap_hir::Function, std::collections::HashSet<ra_ap_hir::Function>>,
}
impl CallGraph {
    fn add_edge(&mut self, caller: ra_ap_hir::Function, callee: ra_ap_hir::Function) {
        self.nodes.insert(caller);
        self.nodes.insert(callee);
        self.edges.entry(caller).or_insert_with(std::collections::HashSet::new).insert(callee);
    }
}

fn print_all_traits(db: &RootDatabase, def: ra_ap_hir::ModuleDef) {
    match &def {
        ra_ap_hir::ModuleDef::Trait(t) => {
            println!("Trait: {} id:{:?}", t.name(db).display(db, ra_ap_ide::Edition::Edition2024), t);
        }
        _ => {}
    }
}

fn walk_dependency(collect: &mut Collect, db: &RootDatabase) {
    let mut walkqueue = Vec::<ra_ap_hir::Function>::new();
    for (f, val) in collect.functions.iter_mut().filter(|(_, v)| v.force_visited){
        walkqueue.push(f.clone());
        val.visited();
    }
    let sema = Semantics::new(db);
    fn queue_function(db: &RootDatabase, f: ra_ap_hir::Function, collect: &mut Collect, walkqueue: &mut Vec<ra_ap_hir::Function>) {
        fn require_visit(f: &ra_ap_hir::Function, collect: &mut Collect) -> bool {
            if let Some(is_visited) = collect.functions.get_mut(f) {
                let ret = !is_visited.is_visited();
                is_visited.visited();
                ret
            } else {
                false
            }
        }
        fn visit_all_trait_implementers(db: &RootDatabase, function_name: &ra_ap_hir::Name, trait_: &ra_ap_hir::Trait, collect: &mut Collect, walkqueue: &mut Vec<ra_ap_hir::Function>) {
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
            for expr in b.value.syntax().descendants().filter_map(ast::CallableExpr::cast) {
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
        }

    }
}

fn print_all_dependencies(callgraph: &mut CallGraph, db: &RootDatabase, def : ra_ap_hir::ModuleDef) {
    let sema = Semantics::new(db);
        
    match &def {
        ra_ap_hir::ModuleDef::Function(f) => {
            let name = f.name(db);
            println!("Function: {} id:{:?}", name.display(db, ra_ap_ide::Edition::Edition2024), f);
            for ty in DefWithBody::Function(*f).expression_types(db) {  
                println!("  related types: {:?}", ty);
            }  
            if let Some(s) = sema.source(*f)
                && let Some(b) = s.value.body() {
                for expr in b.syntax().descendants()
                    .filter_map(ast::CallableExpr::cast) {
                    match expr {
                        ast::CallableExpr::Call(call) => {
                            if let Some(expr) = call.expr() {
                                if let Some(ty) = sema.type_of_expr(&expr) {
                                    if let Some(callable) = ty.adjusted().as_callable(db) {
                                        // Local function call.
                                        if let ra_ap_hir::CallableKind::Function(resolved) = callable.kind() {  
                                            if let Some(ai) = resolved.as_assoc_item(sema.db) {
                                                match ai.container(sema.db) {
                                                    ra_ap_hir::AssocItemContainer::Impl(impl_) => {
                                                        let ty = impl_.self_ty(sema.db);
                                                        println!("  {:?} -C> {} id:{:?} in impl {}", expr.syntax().text(), resolved.name(db).display(db, ra_ap_ide::Edition::Edition2024), resolved, ty.display(db, ra_ap_hir::DisplayTarget::from_crate(db, ty.krate(db).into())));
                                                    }
                                                    ra_ap_hir::AssocItemContainer::Trait(trait_) => {
                                                        println!("  {:?} -C> {} id:{:?} in trait {}", expr.syntax().text(), resolved.name(db).display(db, ra_ap_ide::Edition::Edition2024), resolved, trait_.name(db).display(db, ra_ap_ide::Edition::Edition2024));
                                                    }
                                                }
                                            }
                                            else 
                                            {
                                                // func is the callee  
                                                println!("  {:?} -C> {} id:{:?} in module {:?}", expr.syntax().text(), resolved.name(db).display(db, ra_ap_ide::Edition::Edition2024), resolved, resolved.module(db).name(db));
                                            }
                                            callgraph.add_edge(f.clone(), resolved.clone());
                                        }   else {
                                            println!("  {:?} -C> Something else", expr.syntax().text());
                                        }
                                    } else  {
                                        println!(" NOT CALLABLE {}", expr.syntax().text());
                                    }
                                } else {
                                    println!("  -C> <unresolved> {}", expr.syntax().text());
                                }
                            }
                        }
                        ast::CallableExpr::MethodCall(call) => {  
                            let func = ra_ap_hir::attach_db(db, || {sema.resolve_method_call(&call)});
                            if let Some(resolved) = func {
                                println!("  {:?} -MC> {} id:{:?} in module {:?}", call.syntax().text(), resolved.name(db).display(db, ra_ap_ide::Edition::Edition2024), resolved, resolved.module(db).name(db));
                                callgraph.add_edge(f.clone(), resolved.clone());
                            } else {
                                println!("  {:?} -MC> <unresolved> {}", call.syntax().text(), call.syntax().text());
                            }
                            // func is the callee  
                        }
                    }
                }
            }
            
        }
        ra_ap_hir::ModuleDef::Module(m) => {
            for sub_def in m.declarations(db) {
                print_all_dependencies(callgraph, db, sub_def);
            }
        }
        _ => {}
    }
}