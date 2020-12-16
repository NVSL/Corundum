
use proc_macro2::Group;
use crate::syn::parse::Parser;
use syn::punctuated::Punctuated;
use proc_macro2::TokenStream;
use quote::{quote, quote_spanned, format_ident};
use syn::spanned::Spanned;
use syn::*;

pub fn derive_pclone(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    // Parse the input tokens into a syntax tree.
    let input = parse_macro_input!(input as DeriveInput);

    let pools = pools(input.attrs);

    // Used in the quasi-quotation below as `#name`.
    let name = input.ident;

    let mut expanded = vec![];
    for p in &pools {

        // Add a bound `T: PClone` to every type parameter T.
        let generics = add_trait_bounds(input.generics.clone(), &pools, &p);
        let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

        // Generate an expression to sum up the heap size of each field.
        let sum = pclone_all_fields(&name, &input.data);

        expanded.push(quote! {
            #[automatically_derived]
            #[allow(unused_qualifications)]
            impl#impl_generics corundum::clone::PClone<#p> for #name #ty_generics #where_clause {
                #[inline]
                fn pclone(&self, j: &corundum::stm::Journal<#p>) -> Self {
                    #sum
                }
            }
        });
    }

    let expanded = quote! { #(#expanded)* };

    // Hand the output tokens back to the compiler.
    proc_macro::TokenStream::from(expanded)
}

fn pools(attrs: Vec<Attribute>) -> Vec<TokenStream> {
    let mut p = vec![];
    for attr in &attrs {
        for segment in attr.path.segments.iter() {
            if segment.ident.to_string() == String::from("pools") {
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

// Add a bound `T: PClone` to every type parameter T.
fn add_trait_bounds(mut generics: Generics, pool: &Vec<TokenStream>, p: &TokenStream) -> Generics {
    for param in &mut generics.params {
        if let GenericParam::Type(ref mut type_param) = *param {
            let ident = type_param.ident.clone();
            let me = ident.to_string();
            if !pool.iter().any(|p| p.to_string() == me) {
                type_param.bounds.push(parse_quote!(corundum::clone::PClone<#p>));
            }
        }
    }
    generics
}

// Generate an expression to sum up the heap size of each field.
fn pclone_all_fields(ident: &Ident, data: &Data) -> TokenStream {
    match *data {
        Data::Struct(ref data) => {
            match data.fields {
                Fields::Named(ref fields) => {
                    let recurse = fields.named.iter().map(|f| {
                        let name = &f.ident;
                        quote_spanned! {f.span()=>
                            #name: corundum::clone::PClone::pclone(&self.#name, j)
                        }
                    });
                    quote! {
                        Self {
                            #(#recurse,)* 
                        }
                    }
                }
                Fields::Unnamed(ref fields) => {
                    // Expands to an expression like
                    //
                    //     0 + self.0.heap_size() + self.1.heap_size() + self.2.heap_size()
                    let recurse = fields.unnamed.iter().enumerate().map(|(i, f)| {
                        let index = Index::from(i);
                        quote_spanned! {f.span()=>
                            corundum::clone::PClone::pclone(&self.#index, j)
                        }
                    });
                    quote! {
                        Self(#(#recurse,)*)
                    }
                }
                Fields::Unit => {
                    // Unit structs cannot own more than 0 bytes of heap memory.
                    quote!(0)
                }
            }
        }
        Data::Enum(DataEnum { ref variants, .. }) => {
            let res = variants.iter().map(|ref v| {
                let variant = v.ident.clone();
                match v.fields {
                    Fields::Unit => quote! {
                        #ident::#variant => #ident::#variant
                    },
                    Fields::Unnamed(ref fields) => {
                        let recurse = fields.unnamed.iter().enumerate().map(|(i, f)| {
                            let varname = format_ident!("__self_{}", i);
                            quote_spanned! {f.span()=>
                                #varname
                            }
                        });
                        let clones = recurse.clone();
                        quote! {
                            #ident::#variant(#(#recurse,)*) => 
                                #ident::#variant(#(corundum::clone::PClone::pclone(&#clones, j),)*)
                        }
                    },
                    Fields::Named(ref fields) => {
                        let recurse = fields.named.iter().enumerate().map(|(i,f)| {
                            let name = &f.ident;
                            let varname = format_ident!("__self_{}", i);
                            quote_spanned! {f.span()=>
                                #name: #varname
                            }
                        });
                        let clones = fields.named.iter().enumerate().map(|(i,f)| {
                            let name = &f.ident;
                            let varname = format_ident!("__self_{}", i);
                            quote_spanned! {f.span()=>
                                #name: corundum::clone::PClone::pclone(&#varname, j)
                            }
                        });
                        quote! {
                            #ident::#variant{#(#recurse,)*} => 
                            #ident::#variant{#(#clones,)*}
                        }
                    }
                }
            });
            quote! {
                match self {
                    #(#res,)* 
                }
            }
        }
        Data::Union(_) => panic!("Union types cannot derive PClone"),
    }
}