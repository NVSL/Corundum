#![feature(once_cell)]
#![feature(type_name_of_val)]
#![feature(proc_macro_span)]

use proc_macro2::Group;
use syn::parse::Parser;
use syn::punctuated::Punctuated;
use proc_macro::TokenStream;
use quote::quote;
use syn::*;

extern crate syn;
extern crate proc_macro;
extern crate quote;
extern crate cbindgen;

#[macro_use]
extern crate proc_macro_error;

mod pclone;
mod root;
mod cbinding;

#[proc_macro_error]
#[proc_macro_derive(PClone, attributes(pools))]
pub fn derive_pclone(input: TokenStream) -> TokenStream {
    pclone::derive_pclone(input)
}

#[proc_macro_error]
#[proc_macro_derive(Root, attributes(pools))]
pub fn derive_root(input: TokenStream) -> TokenStream {
    root::derive_root(input)
}

#[proc_macro_error]
#[proc_macro_derive(Export, attributes(mods,attrs))]
pub fn derive_cbindgen(input: TokenStream) -> TokenStream {
    cbinding::derive_cbindgen(input)
}

#[proc_macro_error]
#[proc_macro_attribute]
pub fn export(attr: TokenStream, item: TokenStream) -> TokenStream {
    cbinding::cbindgen(attr, item)
}

fn list(attrs: &Vec<Attribute>, name: &str) -> Vec<proc_macro2::TokenStream> {
    let mut ret = vec![];
    for attr in attrs {
        for segment in attr.path.segments.iter() {
            if segment.ident.to_string() == String::from(name) {
                if let Ok(g) = parse2::<Group>(attr.tokens.clone()) {
                    let parser = Punctuated::<Path, Token![,]>::parse_terminated;
                    if let Ok(list) = parser.parse2(g.stream()) {
                        for item in list {
                            ret.push(quote! { #item });
                        }
                    }
                }
            };
        }
    }
    if ret.is_empty() && name == "pools" {
        vec![quote!{ corundum::default::Allocator }]
    } else {
        ret
    }
}

#[proc_macro_error]
#[proc_macro]
pub fn carbide(input: TokenStream) -> TokenStream {
    cbinding::carbide(input)
}

#[proc_macro_error]
#[proc_macro]
pub fn generate(input: TokenStream) -> TokenStream {
    use spanned::Spanned;
    use std::path::PathBuf;

    let mut overwrite = false;
    let mut warning = true;
    let mut dir: Option<(PathBuf,proc_macro2::Span)> = None;

    
    let parser = Punctuated::<Expr, Token![,]>::parse_terminated;
    if let Ok(list) = parser.parse2(input.into()) {
        for item in list {
            if let Expr::Assign(ass) = item {
                if !if let Expr::Path(p) = &*ass.left {
                    if p.path.segments.len() == 1 {
                        match &p.path.segments[0].ident.to_string()[..] {
                            "path" => {
                                if !if let Expr::Lit(p) = &*ass.right {
                                    if let Lit::Str(d) = &p.lit {
                                        let dr = PathBuf::from(d.value());
                                        dir = Some((dr,d.span()));
                                        true
                                    } else {
                                        false
                                    }
                                } else {
                                    false
                                } {
                                    abort!(ass.right.span(), "invalid value";
                                        note = "aborting export procedure";
                                        help = "specify a valid string"
                                    );
                                }
                                true
                            }
                            "overwrite" => {
                                if !if let Expr::Lit(p) = &*ass.right {
                                    if let Lit::Bool(b) = &p.lit {
                                        overwrite = b.value;
                                        true
                                    } else {
                                        false
                                    }
                                } else {
                                    false
                                } {
                                    abort!(ass.right.span(), "invalid value";
                                        note = "aborting export procedure";
                                        help = "specify a valid bool (true/false)"
                                    );
                                }
                                true
                            }
                            "warning" => {
                                if !if let Expr::Lit(p) = &*ass.right {
                                    if let Lit::Bool(b) = &p.lit {
                                        warning = b.value;
                                        true
                                    } else {
                                        false
                                    }
                                } else {
                                    false
                                } {
                                    abort!(ass.right.span(), "invalid value";
                                        note = "aborting export procedure";
                                        help = "specify a valid bool (true/false)"
                                    );
                                }
                                true
                            }
                            _ => { false }
                        }
                    } else {
                        false
                    }
                } else {
                    false
                } {
                    abort!(ass.left.span(), "invalid option";
                        note = "aborting export procedure";
                        note = "available options are 'path', 'overwrite', and 'warning'"
                    );
                }
            } else {
                abort!(item.span(), "invalid option";
                    note = "aborting export procedure";
                    note = "available options are 'path', 'overwrite', and 'warning'"
                );
            }
        }
    }

    if let Some((dir,span)) = dir {
        if let Err(err) = cbinding::export(dir, span, overwrite, warning) {
            abort_call_site!(
                "header files generation failed";
                note = "{}", err;
            );
        }
    }

    TokenStream::default()
}