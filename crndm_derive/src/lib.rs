use proc_macro2::Group;
use syn::parse::Parser;
use syn::punctuated::Punctuated;
use proc_macro::TokenStream;
use quote::quote;
use syn::*;

extern crate syn;
extern crate proc_macro;
extern crate quote;

mod pclone;
mod root;
mod cbindgen;

#[proc_macro_derive(PClone, attributes(pools))]
pub fn derive_pclone(input: TokenStream) -> TokenStream {
    pclone::derive_pclone(input)
}

#[proc_macro_derive(Root, attributes(pools))]
pub fn derive_root(input: TokenStream) -> TokenStream {
    root::derive_root(input)
}

#[proc_macro_derive(PoolCBindGen, attributes(mods,container,open_flags))]
pub fn derive_poolcbindgen(input: TokenStream) -> TokenStream {
    cbindgen::derive_poolcbindgen(input)
}

#[proc_macro_derive(CBindGen, attributes(mods,container))]
pub fn derive_cbindgen(input: TokenStream) -> TokenStream {
    cbindgen::derive_cbindgen(input)
}

#[proc_macro_attribute]
pub fn cbindgen(attr: TokenStream, item: TokenStream) -> TokenStream {
    cbindgen::cbindgen(attr, item)
}

fn pools(attrs: Vec<Attribute>, name: &str) -> Vec<proc_macro2::TokenStream> {
    let mut p = vec![];
    for attr in &attrs {
        for segment in attr.path.segments.iter() {
            if segment.ident.to_string() == String::from(name) {
                if let Ok(g) = parse2::<Group>(attr.tokens.clone()) {
                    let parser = Punctuated::<Path, Token![,]>::parse_terminated;
                    if let Ok(pools) = parser.parse2(g.stream()) {
                        for pool in pools {
                            p.push(quote! { #pool });
                        }
                    }
                }
            };
        }
    }
    if p.is_empty() {
        vec![quote!{ corundum::default::BuddyAlloc }]
    } else {
        p
    }
}