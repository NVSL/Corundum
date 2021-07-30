use proc_macro2::Group;
use proc_macro::TokenStream;
use syn::parse::Parser;
use proc_macro2::TokenStream as TokenStream2;
use quote::{quote, format_ident};
use syn::*;
use syn::punctuated::Punctuated;

pub fn derive_poolcbindgen(input: TokenStream) -> TokenStream {
    // Parse the input tokens into a syntax tree.
    let input = parse_macro_input!(input as DeriveInput);

    let mods = crate::pools(input.attrs.clone(), "mods");
    let flags = open_flags(input.attrs.clone());

    // Used in the quasi-quotation below as `#name`.
    // let name = input.ident;

    let mut expanded = vec![];
    for m in &mods {

        let container = container(input.attrs.clone(), m.clone());

        // // Generate an expression to sum up the heap size of each field.
        // let sum = root_all_fields(&name, &input.data);
        let name_str: String = m.to_string();
        let fn_open = format_ident!("{}_open", name_str);
        let fn_close = format_ident!("{}_close", name_str);
        let fn_base = format_ident!("{}_base", name_str);
        let fn_alloc = format_ident!("{}_alloc", name_str);
        let fn_dealloc = format_ident!("{}_dealloc", name_str);
        let fn_allocated = format_ident!("{}_allocated", name_str);
        let fn_valid = format_ident!("{}_valid", name_str);
        let fn_txn_begin = format_ident!("{}_txn_begin", name_str);
        let fn_txn_commit = format_ident!("{}_txn_commit", name_str);
        let fn_txn_rollback = format_ident!("{}_txn_rollback", name_str);
        let fn_journal = format_ident!("{}_journal", name_str);
        let fn_log = format_ident!("{}_log", name_str);
        let fn_print_info = format_ident!("{}_print_info", name_str);
        let fn_read64 = format_ident!("{}_read64", name_str);
        let mod_name = format_ident!("__{}", name_str);
        let root_name = format_ident!("PMem_{}", name_str);

        expanded.push(quote! {
            pub mod #mod_name {
                use super::*;
                use corundum::stm::{Logger, Notifier};
                use corundum::stl::HashMap as PHashMap;
                use core::ffi::c_void;
                use std::os::raw::c_char;
                use std::ffi::CStr;
                use #m::*;

                pub struct #root_name {
                    pub(crate) objs: PMutex<PHashMap<u64, #container <BuddyAlloc>, BuddyAlloc>>
                }

                impl RootObj<BuddyAlloc> for #root_name {
                    fn init(j: &Journal) -> Self {
                        Self {
                            objs: PMutex::new(PHashMap::new(j))
                        }
                    }
                }

                #[no_mangle]
                pub extern "C" fn #fn_open(path: *const c_char) -> *const #root_name {
                    let path = unsafe { CStr::from_ptr(path).to_str().unwrap() };
                    let res = BuddyAlloc::open::<#root_name>(path, #flags).unwrap();
                    let p = &*res as *const #root_name;
                    std::mem::forget(res); // Keep the pool open
                    p
                }

                #[no_mangle]
                pub extern "C" fn #fn_close() -> bool {
                    if BuddyAlloc::is_open() {
                        unsafe { BuddyAlloc::close().unwrap(); }
                        true
                    } else {
                        false
                    }
                }

                #[no_mangle]
                pub extern "C" fn #fn_base() -> usize {
                    BuddyAlloc::start() as usize
                }

                #[no_mangle]
                pub extern "C" fn #fn_alloc(size: usize) -> *mut c_void {
                    let j = unsafe {
                        Journal::current(false)
                        .expect("pool_alloc cannot be used outside a transaction")
                    };
                    unsafe { BuddyAlloc::new_uninit_for_layout(size, &*j.0) as *mut c_void }
                }

                #[no_mangle]
                pub extern "C" fn #fn_dealloc(ptr: *mut c_void, size: usize) {
                    assert!(
                        Journal::is_running(),
                        "pool_dealloc cannot be used outside a transaction"
                    );
                    unsafe { BuddyAlloc::free_slice(std::slice::from_raw_parts_mut(ptr, size)); }
                }

                #[no_mangle]
                pub extern "C" fn #fn_allocated(off: u64, size: usize) -> bool {
                    if BuddyAlloc::allocated(off, size) || off == 0 {
                        true
                    } else {
                        eprintln!("off {} (len: {}) is not allocated", off, size);
                        false
                    }
                }
                #[no_mangle]
                pub extern "C" fn #fn_valid(ptr: *const c_void) -> bool {
                    BuddyAlloc::valid(ptr)
                }

                #[no_mangle]
                pub extern "C" fn #fn_txn_begin() -> *const c_void {
                    unsafe {
                        let j = Journal::current(true).unwrap();
                        *j.1 += 1;
                        let journal = utils::as_mut(j.0);
                        journal.unset(corundum::stm::JOURNAL_COMMITTED);
                        journal as *const _ as *const u8 as *const c_void
                    }
                }

                #[no_mangle]
                pub extern "C" fn #fn_txn_commit() {
                    unsafe {
                        corundum::ll::sfence();
                        BuddyAlloc::commit();
                    }
                }

                #[no_mangle]
                pub extern "C" fn #fn_txn_rollback() {
                    unsafe {
                        corundum::ll::sfence();
                        if BuddyAlloc::rollback() {
                            eprintln!("note: transaction rolled back successfully");
                        }
                    }
                }

                #[no_mangle]
                pub extern "C" fn #fn_journal() -> *const c_void {
                    unsafe {
                        if let Some(j) = Journal::current(false) {
                            let journal = utils::as_mut(j.0);
                            journal as *const _ as *const u8 as *const c_void
                        } else {
                            std::ptr::null()
                        }
                    }
                }

                #[no_mangle]
                pub extern "C" fn #fn_log(obj: *const c_void, logged: *const u8, size: usize, j: *const c_void) {
                    assert!(!obj.is_null() && !j.is_null());
                    unsafe {
                        if BuddyAlloc::valid(obj) {
                            let slice = std::slice::from_raw_parts(obj as *mut u8, size);
                            slice.create_log(
                                corundum::utils::read::<Journal>(j as *mut u8),
                                if logged.is_null() {
                                    Notifier::None
                                } else {
                                    Notifier::NonAtomic(Ptr::from_raw(logged))
                                }
                            );
                        }
                    }
                }

                #[no_mangle]
                pub extern "C" fn #fn_print_info() {
                    BuddyAlloc::print_info()
                }

                #[no_mangle]
                pub extern "C" fn #fn_read64(addr: u64) -> u64 {
                    unsafe { *BuddyAlloc::get_unchecked(addr) }
                }
            }
            pub type #root_name = #mod_name::#root_name;
        });
    }

    let expanded = quote! { #(#expanded)* };

    // Hand the output tokens back to the compiler.
    TokenStream::from(expanded)
}

pub fn derive_cbindgen(input: TokenStream) -> TokenStream {
    // Parse the input tokens into a syntax tree.
    let input = parse_macro_input!(input as DeriveInput);

    let mods = crate::pools(input.attrs.clone(), "mods");

    // Used in the quasi-quotation below as `#name`.
    let name = input.ident;

    let mut expanded = vec![];
    for m in &mods {

        // // Generate an expression to sum up the heap size of each field.
        // let sum = root_all_fields(&name, &input.data);

        let __m = format_ident!("PMem_{}", m.to_string());
        let mod_mangled = m.to_string().replace("::", "_").replace("<", "_").replace(">", "_").replace(" ", "").to_lowercase();
        let name_str = format!("__{}_{}", mod_mangled, name.to_string().to_lowercase());
        let fn_new = format_ident!("{}_new", name_str);
        let fn_drop = format_ident!("{}_drop", name_str);
        let fn_open = format_ident!("{}_open", name_str);
        let container = container(input.attrs.clone(), m.clone());

        expanded.push(quote! {

            #[no_mangle]
            pub extern "C" fn #fn_new(j: *const c_void) -> *const #name<#m::BuddyAlloc> {
                use corundum::boxed::Pbox;

                assert!(!j.is_null(), "transactional operation outside a transaction");
                unsafe {
                    let j = corundum::utils::read::<corundum::stm::Journal<#m::BuddyAlloc>>(j as *mut u8);
                    Pbox::leak(Pbox::new(Stack::new(), j)) as *mut #name<#m::BuddyAlloc>
                }
            }

            #[no_mangle]
            pub extern "C" fn #fn_drop(obj: *mut #name<#m::BuddyAlloc>) {
                use corundum::boxed::Pbox;

                assert!(!obj.is_null(),);
                unsafe {
                    Pbox::<#name<#m::BuddyAlloc>,#m::BuddyAlloc>::from_raw(obj); // drops when out of scope
                }
            }

            #[no_mangle]
            pub extern "C" fn #fn_open(p: &#__m, name: *const c_char) -> *const #name<#m::BuddyAlloc> {
                let name = unsafe { CStr::from_ptr(name).to_str().unwrap() };
                let mut hasher = DefaultHasher::new();
                name.hash(&mut hasher);
                let key = hasher.finish();

                let mut res: *const #name<#m::BuddyAlloc> = std::ptr::null();
                if transaction(AssertTxInSafe(|j| {
                    let mut objs = p.objs.lock(j);
                    let obj = objs.get_or_insert(key, || {
                        #container::<#m::BuddyAlloc>::#name(#name::new())
                    }, j);
                    if let #container::<#m::BuddyAlloc>::#name(obj) = &obj {
                        res = obj as *const #name<#m::BuddyAlloc>;
                    }
                })).is_err() {
                    res = std::ptr::null();
                }
                res
            }
        });
    }

    let expanded = quote! { #(#expanded)* };

    // Hand the output tokens back to the compiler.
    TokenStream::from(expanded)
}

fn container(attrs: Vec<Attribute>, m: TokenStream2) -> TokenStream2 {
    // Should be only one type with exactly one type parameter of type MemPool
    for attr in &attrs {
        for segment in attr.path.segments.iter() {
            if segment.ident.to_string() == String::from("container") {
                if let Ok(g) = parse2::<Group>(attr.tokens.clone()) {
                    return g.stream();
                }
            };
        }
    }
    quote!{ corundum::gen::ByteObject<#m::BuddyAlloc> }
}

fn open_flags(attrs: Vec<Attribute>) -> TokenStream2 {
    let mut vflags = vec![];
    for attr in &attrs {
        for segment in attr.path.segments.iter() {
            if segment.ident.to_string() == String::from("open_flags") {
                if let Ok(g) = parse2::<Group>(attr.tokens.clone()) {
                    let parser = Punctuated::<Path, Token![,]>::parse_terminated;
                    if let Ok(flags) = parser.parse2(g.stream()) {
                        for flag in flags {
                            vflags.push(quote! { #flag });
                        }
                    }
                }
            };
        }
    }
    if vflags.is_empty() { vflags.push(quote! { O_CFNE }) }
    quote!{ #(#vflags)|* }
}

use crate::quote::ToTokens;

pub fn cbindgen(attr: TokenStream, item: TokenStream) -> TokenStream {
    eprintln!("parse file: {}", attr.to_string());
    if let Ok(input) = parse2::<ItemImpl>(item.clone().into()) {
        if let Type::Path(tp) = *input.self_ty {
            let mut tokens = TokenStream2::default();
            tp.path.to_tokens(&mut tokens);
            eprintln!("Type: {}", tokens);
        }
        for i in input.items {
            let mut tokens = TokenStream2::default();
            i.to_tokens(&mut tokens);
            if let Ok(f) = parse2::<ItemFn>(tokens) {
                eprintln!("function: \"{}\"", f.sig.ident.to_string());
            }
            
        }
    }
    item
}