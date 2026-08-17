use std::{cell::Cell, collections::HashMap, fmt::Debug};

use ra_ap_hir::{AsAssocItem, Crate};
use ra_ap_ide::{RootDatabase, Semantics};
use ra_ap_syntax::{ast, AstNode};

/// Visit Information for HIR
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
        Self { visited : Cell::new(visited) }
    }

    /// Set the `visited` flag to `true` and return the previous value of the `visited` flag.
    fn visit(&self) -> bool {
        self.visited.replace(true)
    }

    /// Check whether the item is visited or force_visited.
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
        for m in root_krate.modules(db) {
            found_main |= result.do_module(m, &sema, db, &entrypoint_name);
        }
        found_main.then_some(result)
    }

    fn do_module<S: AsRef<str>>(&mut self, m: ra_ap_hir::Module, sema: &Semantics<RootDatabase>, db: &RootDatabase, entrypoint_name: &Option<S>) -> bool {
        let mut found_main = false;
        for imp in m.impl_defs(db) {
            let force_visited = imp.trait_(db).is_some();
            for i in imp.items(db).iter() {
                match i {
                    ra_ap_hir::AssocItem::Function(f) => {
                        self.functions.insert(f.clone(), HirVisit::new(force_visited));
                    }
                    ra_ap_hir::AssocItem::Const(c) => {
                        self.consts.insert(c.clone(), HirVisit::new(force_visited));
                    }
                    ra_ap_hir::AssocItem::TypeAlias(_) => {
                        // DO NOTHING
                    }
                }
            }
        }

        for d in m.declarations(db) {
            match d {
                ra_ap_hir::ModuleDef::Module(sub_m) => {
                    found_main |= self.do_module(sub_m, sema, db, entrypoint_name);
                }
                ra_ap_hir::ModuleDef::Function(f) => {
                    let is_visited = if let Some(entry_name) = entrypoint_name {
                        f.name(db).as_str() == entry_name.as_ref()
                    } else {
                        f.is_main(db)
                    };
                    found_main |= is_visited;
                    self.functions.insert(f, HirVisit::new(is_visited));
                }
                ra_ap_hir::ModuleDef::Adt(adt) => {
                    self.adts.insert(adt, HirVisit::force_visited());
                }
                ra_ap_hir::ModuleDef::Const(c) => {
                    self.consts.insert(c, HirVisit::force_visited());
                }
                ra_ap_hir::ModuleDef::Static(st) => {
                    self.statics.insert(st, HirVisit::force_visited());
                }
                ra_ap_hir::ModuleDef::Trait(t) => {
                    self.traits.insert(t, HirVisit::force_visited());
                    for item in t.items(db) {
                        if let ra_ap_hir::AssocItem::Function(f) = item
                            && sema.source(f.clone()).and_then(|s| s.value.body()).is_some() {
                            self.functions.insert(f, HirVisit::new(true));
                        }
                    }
                }
                _ => {}
            }
        }
        found_main
    }

    /// Walk the dependency.
    pub(super) fn walk_dependency(&self, db: &RootDatabase) {
        let mut walkqueue = Vec::<ra_ap_hir::Function>::new();
        for (f, val) in self.functions.iter().filter(|(_, v)| v.is_visited()) {
            walkqueue.push(f.clone());
            val.visit();
        }
        let sema = Semantics::new(db);

        // Scan initializers of consts and statics
        for body in self.consts.keys()
            .filter_map(|body| sema.source(*body))
            .filter_map(|s| s.value.body()) {
            self.scan_syntax_for_functions(db, body.syntax(), &mut walkqueue, &sema);
        }
        for body in self.statics.keys()
            .filter_map(|st| sema.source(*st))
            .filter_map(|s| s.value.body()) {
            self.scan_syntax_for_functions(db, body.syntax(),  &mut walkqueue, &sema);
        }

        while let Some(f) = walkqueue.pop().and_then(|f| sema.source(f)) {
            self.scan_syntax_for_functions(db, f.value.syntax(), &mut walkqueue, &sema);
        }
    }

    // Is it required to visit?
    fn require_visit(&self, f: &ra_ap_hir::Function) -> bool {
        self.functions.get(f).map(|is_visited| !is_visited.visit()).unwrap_or(false)
        // unwrap_or(false): Is the function not collected? wonderful.
    }
    // Push all trait implementers
    fn push_all_trait_implementers(&self, db: &RootDatabase, function_name : &ra_ap_hir::Name, trait_: &ra_ap_hir::Trait, walkqueue: &mut Vec<ra_ap_hir::Function>) {
        for imp in ra_ap_hir::Impl::all_for_trait(db, *trait_) {
            for item in imp.items(db).iter() {
                if let ra_ap_hir::AssocItem::Function(func) = item
                    && &func.name(db) == function_name && self.require_visit(&func)
                    // The function name is same and it is required to visit.     
                {
                    walkqueue.push(func.clone());
                }
            }
        }
    }

    // Queue a function to walk if it has not been visited yet
    fn queue_function(&self, db: &RootDatabase, f: ra_ap_hir::Function, walkqueue: &mut Vec<ra_ap_hir::Function>) {
        if self.require_visit(&f) {
            if let Some(assoc) = f.as_assoc_item(db) {
                match assoc.container(db) {
                    ra_ap_hir::AssocItemContainer::Trait(t) => {
                        // trait Foo
                        self.push_all_trait_implementers(db, &f.name(db), &t, walkqueue);
                    }
                    ra_ap_hir::AssocItemContainer::Impl(i) => {
                        if let Some(t) = i.trait_(db) {
                            // impl Foo for Bar
                            self.push_all_trait_implementers(db, &f.name(db), &t, walkqueue);
                        } else {
                            // impl Bar
                            // DO NOTHING
                        }
                    }
                }
            }
            walkqueue.push(f);
        }
    }
    
    fn scan_syntax_for_functions(
        &self,
        db: &RootDatabase,
        syntax: &ra_ap_syntax::SyntaxNode,
        walkqueue: &mut Vec<ra_ap_hir::Function>,
        sema: &Semantics<RootDatabase>
    ) {
        for expr in syntax.descendants().filter_map(ast::CallableExpr::cast) {
            match expr {
                ast::CallableExpr::Call(call) => {
                    if let Some(callable) = call
                        .expr()
                        .and_then(|expr| sema.type_of_expr(&expr))
                        .and_then(|ty| ty.adjusted().as_callable(db))
                    {
                        if let ra_ap_hir::CallableKind::Function(resolved) = callable.kind() {
                            // Check only if it is a function.
                            // TupleStruct and TupleEnumVariant are compiler intrinsics.
                            // Callees of Closure, FnPtr, FnImpl are collected where defined.
                            self.queue_function(db, resolved.clone(), walkqueue);
                        }
                    }
                }
                ast::CallableExpr::MethodCall(call) => {
                    if let Some(resolved) = sema.resolve_method_call(&call) {
                        self.queue_function(db, resolved, walkqueue);
                    }
                }
            }
        }

        // // Binary operators (e.g. a + b -> Add::add, a == b -> PartialEq::eq)
        // for bin_expr in syntax.descendants().filter_map(ast::BinExpr::cast) {
        //     if let Some(resolved) = sema.resolve_bin_expr(&bin_expr) {
        //         self.queue_function(db, resolved, walkqueue);
        //     }
        // }

        // // Unary/Prefix operators (e.g. -a -> Neg::neg, !a -> Not::not, *a -> Deref::deref)
        // for prefix_expr in syntax.descendants().filter_map(ast::PrefixExpr::cast) {
        //     if let Some(resolved) = sema.resolve_prefix_expr(&prefix_expr) {
        //         self.queue_function(db, resolved, walkqueue);
        //     }
        // }

        // // Index operators (e.g. a[i] -> Index::index / IndexMut::index_mut)
        // for index_expr in syntax.descendants().filter_map(ast::IndexExpr::cast) {
        //     if let Some(resolved) = sema.resolve_index_expr(&index_expr) {
        //         self.queue_function(db, resolved, walkqueue);
        //     }
        // }

        // // Try operator (e.g. expr? -> Try::branch / From::from)
        // for try_expr in syntax.descendants().filter_map(ast::TryExpr::cast) {
        //     if let Some(resolved) = sema.resolve_try_expr(&try_expr) {
        //         self.queue_function(db, resolved, walkqueue);
        //     }
        // }

        // // Await operator (e.g. fut.await -> Future::poll)
        // for await_expr in syntax.descendants().filter_map(ast::AwaitExpr::cast) {
        //     if let Some(resolved) = sema.resolve_await_to_poll(&await_expr) {
        //         self.queue_function(db, resolved, walkqueue);
        //     }
        // }

        // Higher-order functions
        for pr in syntax.descendants()
            .filter_map(ast::PathExpr::cast)
            .filter_map(|path_expr| path_expr.path().and_then(|path| sema.resolve_path(&path))) {
            if let ra_ap_hir::PathResolution::Def(ra_ap_hir::ModuleDef::Function(resolved)) = pr {
                self.queue_function(db, resolved, walkqueue);
            }
        }

        // // Implicit Deref Coercions (e.g. function arguments, autoderef on method/field access)
        // for adjust in syntax
        //     .descendants()
        //     .filter_map(ast::Expr::cast)
        //     .filter_map(|expr| sema.expr_adjustments(&expr))
        //     .flatten()
        // {
        //     match adjust.kind {
        //         ra_ap_hir::Adjust::Deref(Some(overloaded))  => {
        //             self.queue_types(&adjust.source, walkqueue, db);
        //             let target_trait = match overloaded.0 {
        //                 ra_ap_hir::Mutability::Shared => "Deref",
        //                 ra_ap_hir::Mutability::Mut => "DerefMut",
        //             };
        //             for item in ra_ap_hir::Impl::all_for_type(db, adjust.source.clone())
        //                 .into_iter()
        //                 .filter(|imp| imp.trait_(db).map(|t| t.name(db).as_str() == target_trait).unwrap_or(false))
        //                 .flat_map(|imp| imp.items(db))
        //             {
        //                 match item {
        //                     ra_ap_hir::AssocItem::Function(func) => {
        //                         self.queue_function(db, func, walkqueue);
        //                     },
        //                     _ => {}
        //                 }
        //             }
        //         },
        //         _ => {}
        //     }
        // }
    }
}
