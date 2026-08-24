use std::{cell::{Cell, RefCell}, collections::{HashMap, HashSet}, fmt::Debug};

use ra_ap_hir::{AsAssocItem, Crate, DefWithBody, Type};
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
    visited_foreign_traits: RefCell<HashSet<ra_ap_hir::Trait>>,
    pub(super) impls: HashMap<ra_ap_hir::Impl, HirVisit>,
    pub(super) adts: HashMap<ra_ap_hir::Adt, HirVisit>,
    pub(super) consts: HashMap<ra_ap_hir::Const, HirVisit>,
    pub(super) statics: HashMap<ra_ap_hir::Static, HirVisit>,
}

impl HirCollect {
    /// Construct 'HirCollect'. Return `None` if the entry point function is not found.
    pub(super) fn new<S: AsRef<str>>(root_krate: &Crate, db: &RootDatabase, entrypoint_name : &Option<S>) -> Option<Self> {
        let mut result = HirCollect {
            functions: HashMap::new(),
            traits: HashMap::new(),
            visited_foreign_traits: RefCell::new(HashSet::new()),
            impls: HashMap::new(),
            adts: HashMap::new(),
            consts: HashMap::new(),
            statics: HashMap::new(),
        };
        let sema = Semantics::new(db);
        let mut found_main = false;
        for m in root_krate.modules(db) {
            found_main |= result.do_module(m, &sema, db, entrypoint_name);
        }
        found_main.then_some(result)
    }

    fn do_module<S: AsRef<str>>(&mut self, m: ra_ap_hir::Module, sema: &Semantics<RootDatabase>, db: &RootDatabase, entrypoint_name: &Option<S>) -> bool {
        for imp in m.impl_defs(db) {
            for i in imp.items(db) {
                match i {
                    ra_ap_hir::AssocItem::Function(f) => {
                        self.functions.insert(f.clone(), HirVisit::new(false));
                    }
                    ra_ap_hir::AssocItem::Const(c) => {
                        self.consts.insert(c.clone(), HirVisit::new(false));
                    }
                    ra_ap_hir::AssocItem::TypeAlias(_) => {
                        // DO NOTHING
                    }
                }
            }
        }

        let mut found_main = false;
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
                    self.adts.insert(adt, HirVisit::new(false));
                }
                ra_ap_hir::ModuleDef::Const(c) => {
                    self.consts.insert(c, HirVisit::force_visited());
                }
                ra_ap_hir::ModuleDef::Static(st) => {
                    self.statics.insert(st, HirVisit::force_visited());
                }
                ra_ap_hir::ModuleDef::Trait(t) => {
                    self.traits.insert(t, HirVisit::new(false));
                    for item in t.items(db) {
                        if let ra_ap_hir::AssocItem::Function(f) = item {
                            self.functions.insert(f, HirVisit::new(false));
                        }
                    }
                }
                _ => {}
            }
        }
        for imp in m.impl_defs(db) {
            self.impls.insert(imp, HirVisit::new(false));
        }
        found_main
    }

    fn is_all_type_used(&self, db: &RootDatabase, ty: &Type<'_>) -> bool {
        if let Some(_) = ty.as_builtin() {
            true
        } else if let Some((adt, args)) = ty.as_adt_with_args() {
            self.adts.get(&adt).map(|v| v.is_visited()).unwrap_or(true) // If the ADT is not found, it is considered used because it is outside the crate. TODO: check it?
            && args.iter().all(|arg| arg.as_ref().map(|arg| self.is_all_type_used(db, &arg)).unwrap_or(false))
        } else if let Some(slice) = ty.as_slice() {
            self.is_all_type_used(db, &slice)
        } else if let Some((ary, _)) = ty.as_array(db) {
            self.is_all_type_used(db, &ary)
        } else if let Some((typ, _)) = ty.as_reference() {
            self.is_all_type_used(db, &typ)
        } else if let Some((typ, _)) = ty.as_raw_ptr() {
            self.is_all_type_used(db, &typ)
        } else if ty.is_unit() {
            true
        } else{
            let tuple_types = ty.tuple_fields(db);
            if tuple_types.len() > 0 {
                tuple_types.iter().all(|t| self.is_all_type_used(db, t))
            } else { // unresolved?
                true // If the type is unresolved or builtin, it is considered used because it is outside the crate. TODO: check it?
            }
        }
    }

    /// Walk the dependency.
    pub(super) fn walk_dependency(&self, db: &RootDatabase, krate: &Crate) {
        fn has_syntactic_trait_target(db: &RootDatabase, impl_: &ra_ap_hir::Impl) -> bool {
            use ra_ap_hir::HasSource;
            impl_.source(db).map(|i| i.value.trait_().is_some()).unwrap_or(false)
        }
        let mut walkqueue = Vec::<ra_ap_hir::Function>::new();
        for (f, val) in self.functions.iter().filter(|(_, v)| v.is_visited()) {
            walkqueue.push(f.clone());
            val.visit();
        }
        let sema = Semantics::new(db);

        // Scan initializers of consts and statics
        for const_ in self.consts.keys() {
            self.queue_types(&const_.ty(db), db);
            if let Some(body) = sema.source(*const_).map(|s| s.value).and_then(|const_| const_.body()) {
                self.scan_syntax_for_functions(db, body.syntax(), &mut walkqueue, &sema);
            }
        }
        for static_ in self.statics.keys(){
            self.queue_types(&static_.ty(db), db);
            if let Some(body) = sema.source(*static_).map(|s| s.value).and_then(|static_| static_.body()) {
                self.scan_syntax_for_functions(db, body.syntax(), &mut walkqueue, &sema);
            }
        }
        while walkqueue.len() > 0 {
            while let Some(f) = walkqueue.pop() {
                if let Some(f) = sema.source(f) {
                    self.scan_syntax_for_functions(db, f.value.syntax(), &mut walkqueue, &sema);
                }
            }
            // Check all newly visited functions in impl.
            for imp in ra_ap_hir::Impl::all_in_crate(db, *krate).iter().filter(|imp| self.impls.contains_key(imp)) {
                let mut visit_all_fns = || {
                    for func in imp.items(db).iter().filter_map(filter_assoc_function) {
                        if self.require_visit(&func) {
                            walkqueue.push(func.clone());
                        }
                    }
                };
                if let Some(trait_) = imp.trait_(db) {
                    if self.is_all_type_used(db, &imp.self_ty(db)) {
                        if !self.traits.contains_key(&trait_) {
                            // If the trait is foreign, all functions are considered used.
                            // Fundamental or operator overloading traits are considered used if the type is used.
                            if self.visited_foreign_traits.borrow().contains(&trait_) 
                            || is_fundamental_or_operator_trait(trait_.name(db).as_str()) {
                                visit_all_fns();
                            }
                        } else {
                            let mut used_functions_name = HashSet::new();
                            // If not foreign, only functions that have been visited are considered used.
                            // Creating used_functions_name
                            for func in trait_.items(db).iter().filter_map(filter_assoc_function) {
                                if self.functions.get(&func).map(|visit| visit.is_visited()).unwrap_or(false) {
                                    used_functions_name.insert(func.name(db));
                                }
                            }
                            // End of creating used_functions_name
                            for func in imp.items(db).iter().filter_map(filter_assoc_function) {
                                if used_functions_name.contains(&func.name(db)) && self.require_visit(&func) {
                                    walkqueue.push(func.clone());
                                }
                            }
                        }
                    }
                } else {
                    if has_syntactic_trait_target(db, &imp) && self.is_all_type_used(db, &imp.self_ty(db)) {
                        visit_all_fns(); // If the trait is unresolved, visit all functions conserveatively.
                    }
                    // Do not queue associated functions.
                }
            }
        }
        // Finaly, mark used traits and impls.
        
        let is_any_item_used = |items : Vec<ra_ap_hir::AssocItem>| {
            items.iter().filter_map(filter_assoc_function).any(|func| {
                self.functions.get(&func).map(|visit| visit.is_visited()).unwrap_or(false)
            })
        };
        for (imp, visit) in self.impls.iter() {
            if self.is_all_type_used(db, &imp.self_ty(db)) && is_any_item_used(imp.items(db)) {
                visit.visit();
            }
        }
        for (trait_, visit) in self.traits.iter() {
            if is_any_item_used(trait_.items(db)) {
                visit.visit();
            }
        }
    }

    // Is it required to visit?
    fn require_visit(&self, f: &ra_ap_hir::Function) -> bool {
        self.functions.get(f).map(|is_visited| !is_visited.visit()).unwrap_or(false)
        // unwrap_or(false): Is the function not collected? wonderful.
    }
    // Push related trait implementers
    fn push_used_trait_implementers(&self, db: &RootDatabase, function_name : &ra_ap_hir::Name, trait_: &ra_ap_hir::Trait, walkqueue: &mut Vec<ra_ap_hir::Function>) {
        if self.traits.contains_key(trait_) { // not foreign_trait
            let mut visit_func = |items : Vec<ra_ap_hir::AssocItem>| {
                for func in items.iter().filter_map(filter_assoc_function) {
                    if &func.name(db) == function_name && self.require_visit(&func) {
                        walkqueue.push(func.clone());
                        break;
                    }
                }
            };
            visit_func(trait_.items(db));
            for imp in ra_ap_hir::Impl::all_for_trait(db, *trait_).iter().filter(|imp| self.is_all_type_used(db, &imp.self_ty(db))) {
                visit_func(imp.items(db));
            }
        } else if self.visited_foreign_traits.borrow().contains(trait_) {
            return; // Already visited
        } else  { // Unvisited foreign trait. Visit all functions of the trait.
            self.visited_foreign_traits.borrow_mut().insert(*trait_);
            for imp in ra_ap_hir::Impl::all_for_trait(db, *trait_).iter().filter(|imp| self.is_all_type_used(db, &imp.self_ty(db))) {
                for func in imp.items(db).iter().filter_map(filter_assoc_function) {
                    if self.require_visit(&func) {
                        walkqueue.push(func.clone());
                    }
                }
            }
        }
    }

    fn queue_type(&self, adt: &ra_ap_hir::Adt, db: &RootDatabase) {
        if let Some(adt_visit) = self.adts.get(adt)
            && adt_visit.visit() {
            match adt {
                ra_ap_hir::Adt::Struct(strct) => {
                    for field in strct.fields(db) {
                        self.queue_types(&field.ty(db), db);
                    }
                }
                ra_ap_hir::Adt::Union(union_) => {
                    for field in union_.fields(db) {
                        self.queue_types(&field.ty(db), db);
                    }
                }
                ra_ap_hir::Adt::Enum(enm) => {
                    for variant in enm.variants(db) {
                        for field in variant.fields(db) {
                            self.queue_types(&field.ty(db), db);
                        }
                    }
                }
            }
        }
    }

    fn queue_types(&self, ty: &Type<'_>, db: &RootDatabase) {
        if let Some((adt, args)) = ty.as_adt_with_args() {
            self.queue_type(&adt, db);
            for arg in args.iter().filter_map(|arg| arg.as_ref()) {
                self.queue_types(arg, db);
            }
        } else if let Some(slice) = ty.as_slice() {
            self.queue_types(&slice, db);
        } else if let Some((ary, _)) = ty.as_array(db) {
            self.queue_types(&ary, db);
        } else if let Some((typ, _)) = ty.as_reference() {
            self.queue_types(&typ, db);
        } else if let Some((typ, _)) = ty.as_raw_ptr() {
            self.queue_types(&typ, db);
        } else if let Some(cal) = ty.as_callable(db) {
            for p in cal.params() {
                self.queue_types(&p.ty(), db);
            }
            self.queue_types(&cal.return_type(), db);
        } else {
            // closure: callables include closures.
            // builtin, unit: no need to queue.
            // trait types: no need to queue because the necessary types have already been queued where they are instantiated.
        }
    }

    // Queue a function to walk if it has not been visited yet
    fn queue_function(&self, db: &RootDatabase, f: ra_ap_hir::Function, walkqueue: &mut Vec<ra_ap_hir::Function>) {
        if self.require_visit(&f) {
            if let Some(assoc) = f.as_assoc_item(db) {
                match assoc.container(db) {
                    ra_ap_hir::AssocItemContainer::Trait(t) => {
                        // trait Foo
                        // It may call all impelementations of the trait function, so queue all implementations whose type is used.
                        self.push_used_trait_implementers(db, &f.name(db), &t, walkqueue);
                    }
                    ra_ap_hir::AssocItemContainer::Impl(i) => {
                        if let Some(t) = i.trait_(db) {
                            // impl Foo for Bar
                            // Conservertively queue all implementations of the trait function whose type is used.
                            self.push_used_trait_implementers(db, &f.name(db), &t, walkqueue);
                        } else {
                            // impl Bar
                            // DO NOTHING
                        }
                    }
                }
            }
            walkqueue.push(f);
            for p in f.assoc_fn_params(db) {
                self.queue_types(&p.ty(), db);
            }
            self.queue_types(&f.ret_type(db), db);

            let f_body : DefWithBody = f.into();
            for ty in f_body.expression_types(db) {
                self.queue_types(&ty, db);
            }
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
    }
}

fn is_fundamental_or_operator_trait(trait_name: &str) -> bool {
    matches!(
        trait_name,
        // Operator overloading traits (std::ops, std::cmp)
        "Add"
            | "Sub"
            | "Mul"
            | "Div"
            | "Rem"
            | "Neg"
            | "Not"
            | "BitAnd"
            | "BitOr"
            | "BitXor"
            | "Shl"
            | "Shr"
            | "AddAssign"
            | "SubAssign"
            | "MulAssign"
            | "DivAssign"
            | "RemAssign"
            | "BitAndAssign"
            | "BitOrAssign"
            | "BitXorAssign"
            | "ShlAssign"
            | "ShrAssign"
            | "Index"
            | "IndexMut"
            | "Deref"
            | "DerefMut"
            | "PartialEq"
            | "Eq"
            | "PartialOrd"
            | "Ord"
            // Fundamental / Standard traits
            | "Drop"
            | "Clone"
            | "Copy"
            | "Default"
            | "Hash"
            | "Debug"
            | "Display"
            | "Send"
            | "Sync"
            | "Unpin"
            | "From"
            | "Into"
            | "TryFrom"
            | "TryInto"
            | "AsRef"
            | "AsMut"
            | "IntoIterator"
            | "Iterator"
            // Future traits
            | "Future"
            | "IntoFuture"
            // Fn
            | "Fn"
            | "FnMut"
            | "FnOnce"
    )
}

fn filter_assoc_function(assoc: &ra_ap_hir::AssocItem) -> Option<ra_ap_hir::Function> {
    if let ra_ap_hir::AssocItem::Function(f) = assoc {
        Some(f.clone())
    } else {
        None
    }
}