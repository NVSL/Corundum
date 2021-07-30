use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{quote, quote_spanned, format_ident};
use syn::spanned::Spanned;
use syn::*;

pub fn derive_root(input: TokenStream) -> TokenStream {
    // Parse the input tokens into a syntax tree.
    let input = parse_macro_input!(input as DeriveInput);

    let pools = crate::pools(input.attrs, "pools");

    // Used in the quasi-quotation below as `#name`.
    let name = input.ident;

    let mut expanded = vec![];
    for p in &pools {

        // Add a bound `T: RootObj` to every type parameter T.
        let generics = add_trait_bounds(input.generics.clone(), &pools, &p);
        let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

        // Generate an expression to sum up the heap size of each field.
        let sum = root_all_fields(&name, &input.data);

        expanded.push(quote! {
            #[automatically_derived]
            #[allow(unused_qualifications)]
            impl#impl_generics corundum::RootObj<#p> for #name #ty_generics #where_clause {
                #[inline]
                fn init(j: &corundum::stm::Journal<#p>) -> Self {
                    #sum
                }
            }
        });
    }

    let expanded = quote! { #(#expanded)* };

    // Hand the output tokens back to the compiler.
    TokenStream::from(expanded)
}

// Add a bound `T: RootObj` to every type parameter T.
fn add_trait_bounds(mut generics: Generics, pool: &Vec<TokenStream2>, p: &TokenStream2) -> Generics {
    for param in &mut generics.params {
        if let GenericParam::Type(ref mut type_param) = *param {
            let ident = type_param.ident.clone();
            let me = ident.to_string();
            if !pool.iter().any(|p| p.to_string() == me) {
                type_param.bounds.push(parse_quote!(corundum::RootObj<#p>));
            }
        }
    }
    generics
}

// Generate an expression to sum up the heap size of each field.
fn root_all_fields(ident: &Ident, data: &Data) -> TokenStream2 {
    match *data {
        Data::Struct(ref data) => {
            match data.fields {
                Fields::Named(ref fields) => {
                    let recurse = fields.named.iter().map(|f| {
                        let name = &f.ident;
                        quote_spanned! {f.span()=>
                            #name: corundum::RootObj::init(j)
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
                    let recurse = fields.unnamed.iter().enumerate().map(|(_, f)| {
                        quote_spanned! {f.span()=>
                            corundum::RootObj::init(j)
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
                        let ints = recurse.clone();
                        quote! {
                            #ident::#variant(#(#recurse,)*) => 
                                #ident::#variant(#(corundum::RootObj::init(&#ints, j),)*)
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
                        let clones = fields.named.iter().enumerate().map(|(_i,f)| {
                            let name = &f.ident;
                            quote_spanned! {f.span()=>
                                #name: corundum::RootObj::init(j)
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
        Data::Union(_) => panic!("Union types cannot derive RootObj"),
    }
}