// The following code is originally based on code from the esp-rs/esp-hal project,
// licensed under the Apache License, Version 2.0 (the "License").

//! This crate provides procedural macros for the `esp-rs-copro` crate.
//! # Features
//! ## For LP coprocessor's project: with `is-lp-core` feature
//! - [`esp_rs_copro_statics`]: Defines the necessary static variables and functions to use the heap allocator from the main processor. You call this macro once.
//! ## For main processor's project: with `has-lp-core` feature
//! - [`define_lp_allocator`]: Defines the necessary static variables and functions to use the heap allocator on the LP coprocessor from the `esp-rs-copro` crate. You call this macro once. Moreover, it must be visible from [`load_lp_code2`].
//! - [`load_lp_code2`]: Load code to be run on the LP/ULP core. This macro is similar to `esp-hal`'s, however it transfers the given value to the LP memory, and the main processor sleeps.
//! ## For shared project
//! - `MovableObject` derive macro. This macro can be used to automatically implement the `MovableObject` trait for a struct or enum, which defines how to move the value to and from the LP memory.

use proc_macro::TokenStream;
use quote::quote;

/// Marks the entry function of a LP core program.
#[cfg(feature = "is-lp-core")]
#[proc_macro_attribute]
pub fn entry(args: TokenStream, item: TokenStream) -> TokenStream {
    use proc_macro2::Span as Span2;
    use proc_macro_crate::FoundCrate;
    #[cfg(not(test))]
    use proc_macro_crate::crate_name;
    use proc_macro2::{Ident, Span};
    use quote::format_ident;
    use syn::{
        FnArg,
        GenericArgument,
        ItemFn,
        PatType,
        PathArguments,
        Type,
        parse::Error,
        spanned::Spanned,
        Pat, PatIdent
    };

    if !args.is_empty() {
        return Error::new(Span::call_site(), "This attribute accepts no arguments")
            .to_compile_error().into();
    }

    // This is a specialized implementation - won't fit other use-cases
    fn to_string(ty: &Type) -> String {
        let mut res = String::new();
        if let Type::Path(p) = ty {
            let segment = p.path.segments.last().unwrap();
            res.push_str(&segment.ident.to_string());

            if let PathArguments::AngleBracketed(g) = &segment.arguments {
                res.push('<');
                let mut pushed = false;
                for arg in &g.args {
                    if pushed {
                        res.push(',');
                    }
                    pushed = true;
                    match arg {
                        GenericArgument::Type(t) => {
                            res.push_str(&to_string(t));
                        },
                        GenericArgument::Const(c) => {
                            res.push_str(&quote! { #c }.to_string());
                        },
                        _ => pushed = false,
                    }
                }
                res.push('>');
            }
        }
        res
    }

    pub(crate) fn make_magic_symbol_name(args: &Vec<&PatType>) -> String {
        let mut res = String::from("__ULP_MAGIC_");
        for &a in args {
            let t = &a.ty;
            let quoted = to_string(t);
            res.push_str(&quoted);
            res.push('$');
        }
        res
    }

    #[cfg(not(test))]
    let found_crate = crate_name("esp-lp-hal").expect("esp-lp-hal is present in `Cargo.toml`");
    #[cfg(test)]
    let found_crate = FoundCrate::Itself;

    let hal_crate = match found_crate {
        FoundCrate::Itself => quote!(esp_lp_hal),
        FoundCrate::Name(name) => {
            let ident = Ident::new(&name, Span::call_site());
            quote!( #ident )
        }
    };


    let input = match syn::parse2::<ItemFn>(item.into()) {
        Ok(f) => f,
        Err(e) => return e.to_compile_error().into(),
    };

    if input.sig.asyncness.is_some() {
        return syn::Error::new(Span2::call_site(), "async functions are not supported by #[entry]")
            .to_compile_error()
            .into();
    }

    let mut arg_exprs = Vec::new();
    let mut args = Vec::new();
    for input in &input.sig.inputs {
        match input {
            FnArg::Typed(pt) => {
                let ty = match pt.ty.as_ref() {
                    Type::Reference(r) if r.mutability.is_some() => r.elem.as_ref(),
                    _ => {
                        return syn::Error::new_spanned(
                            &pt.ty,
                            "#[entry] requires every argument to be of the form `&mut T`",
                        )
                        .to_compile_error()
                        .into();
                    }
                };
                arg_exprs.push(quote! { get_transfer::<#ty>().unwrap() });
                args.push(pt);
            }
            FnArg::Receiver(_) => { // self is not supported currently.
                return syn::Error::new(Span2::call_site(), "methods with self are not supported by #[entry]")
                    .to_compile_error()
                    .into();
            }
        }
    }

    let orig_name = input.sig.ident.clone();
    let magic_symbol_name = make_magic_symbol_name(&args);
    quote! {
        #[allow(non_snake_case)]
        #[unsafe(export_name = "main")]
        pub fn __risc_v_rt__main() {
            #[unsafe(export_name = #magic_symbol_name)]
            static ULP_MAGIC: [u32; 0] = [0u32; 0];
            unsafe { ULP_MAGIC.as_ptr().read_volatile(); }
            #orig_name(#(#arg_exprs),*);
        }
        #input
    }.into()
}

#[cfg(feature="has-lp-core")]
#[proc_macro_attribute]
pub fn entry(args: TokenStream, item: TokenStream) -> TokenStream {
    return quote!{}.into();
}

#[cfg(feature="has-lp-core")]
#[proc_macro]
pub fn load_lp_code3(input: TokenStream) -> TokenStream {
    use std::path::Path;

    use parse::Error;
    use proc_macro::Span;
    use proc_macro2::TokenStream as TokenStream2;
    use proc_macro_crate::{FoundCrate, crate_name};
    use syn::{Ident, LitStr, parse, Token};

    struct LoadLpArgs {
        arch: Option<String>,
        remaining_args: TokenStream2,
    }

    impl parse::Parse for LoadLpArgs {
        fn parse(input: parse::ParseStream) -> parse::Result<Self> {
            let mut arch: Option<String> = None;

            if input.peek(LitStr) {
                let lit: LitStr = input.parse()?;
                let value = lit.value();
                if value.is_empty() {
                    return Err(parse::Error::new(lit.span(), "Architecture string cannot be empty."));
                }
                arch = Some(value);
            }
            
            Ok(Self{ arch, remaining_args: input.parse()?})
        }
    }
    
    let args: LoadLpArgs = match syn::parse(input) {
        Ok(args) => args,
        Err(e) => return e.into_compile_error().into(),
    };

    let archname = if let Some(arch) = args.arch.as_ref() {
        arch
    } else {
        if cfg!(feature = "esp32c6") {
            "riscv32imac-unknown-none-elf"
        } else if cfg!(feature = "esp32s3") || cfg!(feature = "esp32s2") {
            "riscv32imc-unknown-none-elf"
        } else {
            return Error::new(Span::call_site().into(), "Failed to determine target architecture by features flag. Please specify the architecture as a string literal argument, e.g., load_lp_code3!(\"riscv32-imac-unknown-none-elf\").").to_compile_error().into();
        }
    };

    let manifest_dir = if let Ok(mdir) = std::env::var("CARGO_MANIFEST_DIR"){
        mdir
    } else {
        return Error::new(Span::call_site().into(), "Failed to get CARGO_MANIFEST_DIR environment variable.").to_compile_error().into();
    };
    let manifest_path = Path::new(&manifest_dir).join("Cargo.toml");

    let cargo_toml = if let Ok(content) = std::fs::read_to_string(manifest_path) {
        if let Ok(toml) = content.parse::<toml_edit::DocumentMut>() {
            toml
        } else {
            return Error::new(Span::call_site().into(), "Failed to parse Cargo.toml.").to_compile_error().into();
        }
    } else {
        return Error::new(Span::call_site().into(), "Cargo.toml cannot be opened.").to_compile_error().into();
    };

    let lp_path : String ={
        let metadata = if let Some(metadata) = cargo_toml.get("metadata.mtukai")
            && let Some(metadata2) = metadata.as_table() {
            metadata2
        } else {
            return Error::new(Span::call_site().into(), "metadata.mtukai section not found in Cargo.toml. Please make sure to include the mtukai metadata section in your Cargo.toml.").to_compile_error().into();
        };
        if let Some(lp_path) = metadata.get("lp_path")
            && let Some(lp_path_value) =  lp_path.as_str() {
            if let Some(lp_bin_path) = Path::new(&lp_path_value).join("target").join(archname).join("release").join("main").to_str() {
                eprintln!("LP binary path: {}", lp_bin_path);
                lp_bin_path.to_owned()
            } else {
                return Error::new(Span::call_site().into(), "The string value of metadata.mtukai.lp_path in Cargo.toml is invalid.").to_compile_error().into();
            }
        } else {
            return Error::new(Span::call_site().into(), "The string value of metadata.mtukai.lp_path not found in Cargo.toml. Please make sure to include the lp_path field in the mtukai metadata section of your Cargo.toml, pointing to the LP binary generated by the build script.").to_compile_error().into();
        }
    };
    if let Ok(FoundCrate::Name(ref name)) = crate_name("esp-rs-copro-procmacro") {
        let ident = Ident::new(name, Span::call_site().into());
        let path = LitStr::new(&lp_path, Span::call_site().into());
        let ra = args.remaining_args;
        quote!{ #ident::load_lp_code2!(#path #ra) }.into()
    } else { 
        return Error::new(Span::call_site().into(), "esp-rs-copro crate not found. Please add the dependency to your Cargo.toml.").to_compile_error().into();
    }
}