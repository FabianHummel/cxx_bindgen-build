mod cxx_bindgen_meta;
mod cargo_expand;
mod binding_state;

use std::env;
use crate::cxx_bindgen_meta::CxxBindgenMeta;
use build_print::*;
use regex::Regex;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;
use proc_macro_error::emit_error;
use syn::__private::quote::quote;
use syn::{ImplItem, ImplItemFn, Item, ItemEnum, ItemFn, ItemImpl, ItemStruct, Visibility};
use syn::spanned::Spanned;
use crate::binding_state::BindingState;

#[derive(Debug)]
pub struct BridgeBuilder {
    output_file: String,
    namespace: String,
    features: Vec<String>,
}

pub fn bridge(ffi_destination_file: impl AsRef<Path>) -> BridgeBuilder {
    BridgeBuilder {
        output_file: ffi_destination_file.as_ref().to_string_lossy().to_string(),
        namespace: String::new(),
        features: Vec::new(),
    }
}

impl BridgeBuilder {
    pub fn namespace(mut self, namespace: impl Into<String>) -> Self {
        self.namespace = namespace.into();
        self
    }

    pub fn feature(mut self, feature: impl Into<String>) -> Self {
        self.features.push(feature.into());
        self
    }

    pub fn features(mut self, features: Vec<String>) -> Self {
        self.features.extend(features);
        self
    }

    pub fn generate(&self) {
        if env::var("CXX_BINDGEN_RUNNING").is_ok() {
            warn!("Skipping FFI bridge generation because cxx-bindgen is already running.");
            return;
        }

        info!("Generating FFI bridge \"{:?}\"", self);

        let mut bindings = BindingState::default();

        if let Err(error) = self.update_or_create_file(BindingState::default()) {
            warn!("Could not update FFI bindings: {:?}", error);
        }

        let content = cargo_expand::expand(&self.features);

        let ast = syn::parse_file(&content).unwrap();

        self.generate_items(ast.items, &mut bindings);

        if let Err(error) = self.update_or_create_file(bindings) {
            warn!("Could not update FFI bindings: {:?}", error);
        }
    }

    fn generate_items(&self, items: Vec<Item>, bindings: &mut BindingState) {
        for item in items {
            match item {
                Item::Struct(item) => {
                    if !matches!(item.vis, Visibility::Public(_)) {
                        continue;
                    }

                    if let Some(meta) = CxxBindgenMeta::is_processed(&item.attrs) {
                        if meta.skip {
                            continue;
                        }

                        self.generate_ffi_struct(item.clone(), &meta, bindings);
                    }
                }

                Item::Enum(item) => {
                    if !matches!(item.vis, Visibility::Public(_)) {
                        continue;
                    }

                    if let Some(meta) = CxxBindgenMeta::is_processed(&item.attrs) {
                        if meta.skip {
                            continue;
                        }

                        self.generate_ffi_enum(item.clone(), &meta, bindings);
                    }
                }

                Item::Fn(item) => {
                    if !matches!(item.vis, Visibility::Public(_)) {
                        continue;
                    }

                    if let Some(meta) = CxxBindgenMeta::is_processed(&item.attrs) {
                        if meta.skip {
                            continue;
                        }

                        self.generate_ffi_function(item.clone(), &meta, bindings);
                    }
                }

                Item::Impl(item) => {
                    if let Some(meta) = CxxBindgenMeta::is_processed(&item.attrs) {
                        if meta.skip {
                            continue;
                        }

                        self.generate_ffi_impl(item.clone(), &meta, bindings);
                    }
                }

                Item::Mod(item) => {
                    if let Some(meta) = CxxBindgenMeta::is_processed(&item.attrs) {
                        if meta.skip {
                            continue;
                        }
                    }

                    // modules don't need to explicitly attribute cxx_bindgen
                    if let Some((_, items)) = item.content {
                        self.generate_items(items, bindings);
                    }
                }

                _ => {}
            }
        }
    }

    fn generate_ffi_struct(&self, item: ItemStruct, meta: &CxxBindgenMeta, bindings: &mut BindingState) {
        let cxx_attr = meta.cxx_name();

        if meta.shared {
            let mut item = item.clone();
            item.attrs.retain(|attr| attr.path().is_ident("doc"));
            item.vis = Visibility::Inherited;
            writeln!(bindings.shared, "    {}", quote! { #[derive(Serialize,Deserialize)] #cxx_attr #item }).unwrap();
        }
        else {
            let ident = item.ident;
            writeln!(bindings.rust_bindings, "        type {};", quote! { #cxx_attr #ident }).unwrap();
        }
    }

    fn generate_ffi_enum(&self, item: ItemEnum, meta: &CxxBindgenMeta, bindings: &mut BindingState) {
        let cxx_attr = meta.cxx_name();

        if meta.shared {
            let mut item = item.clone();
            item.attrs.retain(|attr| attr.path().is_ident("doc"));
            item.vis = Visibility::Inherited;
            writeln!(bindings.shared, "    {}", quote! { #[derive(Serialize,Deserialize)] #cxx_attr #item }).unwrap();

        }
        else {
            let ident = item.ident;
            writeln!(bindings.rust_bindings, "        type {};", quote! { #cxx_attr #ident }).unwrap();
        }
    }

    fn generate_ffi_function(&self, item: ItemFn, meta: &CxxBindgenMeta, bindings: &mut BindingState) {
        if meta.shared {
            emit_error!(item.sig.span(), "Shared functions are not supported.");
        }

        let mut item = item.clone();
        item.attrs.retain(|attr| attr.path().is_ident("doc"));
        item.vis = Visibility::Inherited;
        item.block.stmts.clear();

        let cxx_attr = meta.cxx_name();

        let quote = quote! {
            #cxx_attr
            #item
        }
            .to_string();

        writeln!(bindings.rust_bindings, "        {};", &quote[..quote.len() - 4]).unwrap(); // Remove { }
    }

    fn generate_ffi_impl(&self, item: ItemImpl, meta: &CxxBindgenMeta, bindings: &mut BindingState) {
        for impl_item in item.items.iter() {
            match impl_item.clone() {
                ImplItem::Fn(mut item_fn) => {
                    if !matches!(item_fn.vis, Visibility::Public(_)) {
                        continue;
                    }

                    let meta = CxxBindgenMeta::is_processed(&item_fn.attrs);

                    if let Some(meta) = meta.clone() {
                        if meta.skip {
                            continue;
                        }
                    }

                    self.generate_ffi_impl_function(&mut item_fn, &meta, &item, bindings);
                }

                _ => {}
            }
        }
    }

    fn generate_ffi_impl_function(&self, item: &mut ImplItemFn, meta: &Option<CxxBindgenMeta>, item_impl: &ItemImpl, bindings: &mut BindingState) {
        item.attrs.retain(|attr| attr.path().is_ident("doc"));
        item.vis = Visibility::Inherited;
        item.block.stmts.clear();

        if let Some(first_arg) = item.sig.inputs.first_mut() {
            if let syn::FnArg::Receiver(receiver) = first_arg {
                let impl_type = item_impl.self_ty.as_ref();
                *first_arg = if receiver.mutability.is_some() {
                    syn::parse_quote! { self: &mut #impl_type }
                } else {
                    syn::parse_quote! { self: &#impl_type }
                };
            }
        }

        let cxx_attr = if let Some(meta) = meta {
            meta.cxx_name()
        } else {
            quote! { }
        };

        let quote = quote! {
            #cxx_attr
            #item
        }
            .to_string();

        writeln!(bindings.rust_bindings, "        {};", &quote[..quote.len() - 4]).unwrap();
    }

    fn update_or_create_file(&self, bindings: BindingState) -> std::io::Result<()> {
        let mut content = String::new();

        if let Ok(mut file) = File::open(self.output_file.clone()) {
            file.read_to_string(&mut content)?;

            let region_regex = Regex::new(r#"// #region "cxx-bridge-generated-shared"[\s\S]*?// #endregion"#).unwrap();

            if region_regex.is_match(&content) {
                content = region_regex.replace(
                    &content,
                    format!(
                        "// #region \"cxx-bridge-generated-shared\"\n{}\n    // #endregion",
                        String::from_utf8(bindings.shared).unwrap(),
                    )
                )
                    .to_string();
            } else {
                panic!(r#"Missing #region "cxx-bridge-generated-shared" block."#);
            }

            let region_regex = Regex::new(r#"// #region "cxx-bridge-generated-rust"[\s\S]*?// #endregion"#).unwrap();

            if region_regex.is_match(&content) {
                content = region_regex.replace(
                    &content,
                    format!(
                        "// #region \"cxx-bridge-generated-rust\"\n{}\n        // #endregion",
                        String::from_utf8(bindings.rust_bindings).unwrap(),
                    ),
                )
                    .to_string();
            } else {
                panic!(r#"Missing #region "cxx-bridge-generated-rust" block."#);
            }
        }
        else {
            content = format!(
                r#"#[cxx::bridge(namespace = "{namespace}")]
mod ffi {{
    // Your custom bindings here

    // #region "cxx-bridge-generated-shared"

    {shared_bindings}

    // #endregion

    extern "Rust" {{
        // Your custom bindings here

        // #region "cxx-bridge-generated-rust"

        {rust_bindings}

        // #endregion
    }}
}}
"#,
                namespace = self.namespace,
                shared_bindings = String::from_utf8(bindings.shared).unwrap(),
                rust_bindings = String::from_utf8(bindings.rust_bindings).unwrap(),
            );
        }

        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(self.output_file.clone())?;
        file.write_all(content.as_bytes())?;
        Ok(())
    }
}