use syn::{Attribute, LitStr};
use syn::__private::quote::__private::TokenStream;
use syn::spanned::Spanned;
use syn::__private::quote::quote;

#[derive(Clone, Debug)]
pub struct CxxBindgenMeta {
    pub skip: bool,
    pub shared: bool,
    pub cxx_name: Option<String>,
}

impl Default for CxxBindgenMeta {
    fn default() -> Self {
        CxxBindgenMeta {
            skip: false,
            shared: false,
            cxx_name: None,
        }
    }
}

impl CxxBindgenMeta {
    pub fn is_processed(attrs: &Vec<Attribute>) -> Option<CxxBindgenMeta> {
        let mut bindgen_meta = CxxBindgenMeta::default();

        if attrs.iter().any(|attr| {
            if attr.meta.path().span().source_text().unwrap().eq("cxx_bindgen::cxx_bindgen_meta") {
                attr.parse_nested_meta(|meta| {
                    if meta.path.is_ident("skip") {
                        bindgen_meta.skip = true;
                        Ok(())
                    }
                    else if meta.path.is_ident("cxx_name") {
                        let value = meta.value()?;
                        let str: LitStr = value.parse()?;
                        bindgen_meta.cxx_name = Some(str.value());
                        Ok(())
                    }
                    else if meta.path.is_ident("shared") {
                        bindgen_meta.shared = true;
                        Ok(())
                    }
                    else {
                        Err(meta.error("unsupported attribute"))
                    }
                }).is_ok()
            }
            else {
                false
            }
        }) {
            return Some(bindgen_meta);
        }

        None
    }

    pub fn cxx_name(&self) -> TokenStream {
        if let Some(cxx_name) = &self.cxx_name {
            quote! { #[cxx_name = #cxx_name] }
        } else {
            quote! {}
        }
    }
}