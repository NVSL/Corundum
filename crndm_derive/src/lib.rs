extern crate syn;
extern crate proc_macro;
extern crate quote;

mod pclone;
mod root;

#[proc_macro_derive(PClone, attributes(pools))]
pub fn derive_pclone(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    pclone::derive_pclone(input)
}

#[proc_macro_derive(Root, attributes(pools))]
pub fn derive_root(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    root::derive_root(input)
}