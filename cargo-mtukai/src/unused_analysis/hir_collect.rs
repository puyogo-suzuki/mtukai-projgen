use std::{cell::Cell, collections::HashMap, fmt::Debug};

use ra_ap_ide::{RootDatabase, Semantics};
use ra_ap_hir::{AsAssocItem, Crate};
use ra_ap_syntax::{AstNode, ast};

/// Visit Infomation for HIR
#[derive(Debug, PartialEq, Eq)]
pub(super) struct HirVisit {
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
    pub(super) fn is_visited(&self) -> bool {
        self.visited.get()
    }
}

#[derive(Debug)]
pub(super) struct HirCollect {
    pub(super) functions: HashMap<ra_ap_hir::Function, HirVisit>,
    pub(super) traits: HashMap<ra_ap_hir::Trait, HirVisit>,
    pub(super) adts: HashMap<ra_ap_hir::Adt, HirVisit>,
    pub(super) consts: HashMap<ra_ap_hir::Const, HirVisit>,
    pub(super) statics: HashMap<ra_ap_hir::Static, HirVisit>,
}

impl HirCollect {
    /// Construct 'HirCollect'. Return `None` if the entry point function is not found.
    pub(super) fn new<S: AsRef<str>>(root_krate: &Crate, db: &RootDatabase, entrypoint_name : Option<S>) -> Option<Self> {
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

    /// Walk the dependency.
    pub(super) fn walk_dependency(&self, db: &RootDatabase) {
        let mut walkqueue = Vec::<ra_ap_hir::Function>::new();
        for (f, val) in self.functions.iter().filter(|(_, v)| v.is_visited()){
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
        for c in self.consts.keys() {
            if let Some(body) = sema.source(*c).and_then(|s| s.value.body()) {
                scan_syntax_for_functions(db, body.syntax(), self, &mut walkqueue, &sema);
            }
        }
        for st in self.statics.keys() {
            if let Some(body) = sema.source(*st).and_then(|s| s.value.body()) {
                scan_syntax_for_functions(db, body.syntax(), self, &mut walkqueue, &sema);
            }
        }

        while let Some(f) = walkqueue.pop() {
            if let Some(b) = sema.source(f) {
                scan_syntax_for_functions(db, b.value.syntax(), self, &mut walkqueue, &sema);
            }
        }
    }
}