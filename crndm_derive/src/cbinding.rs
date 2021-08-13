use proc_macro2::Group;
use proc_macro::TokenStream;
use syn::parse::Parser;
use proc_macro2::TokenStream as TokenStream2;
use quote::{quote, quote_spanned, format_ident};
use syn::spanned::Spanned;
use syn::*;
use syn::punctuated::Punctuated;
use std::collections::HashMap;
use std::lazy::SyncLazy;
use std::sync::Mutex;
use std::io::*;
use std::fs::{File,create_dir_all,read_to_string};
use regex::Regex;

type TypeName = String;
type PoolName = String;
type FuncName = String;
type FuncSig  = String;
type Template  = Vec<String>;
type FuncArgs = Vec<(bool /* is generic */, String)>;

#[derive(Default)]
pub struct Contents {
    contents: String,
    decl: String,
    alias: String,
    traits: HashMap<PoolName, String>,
    funcs: Vec<(FuncName, FuncArgs, FuncSig, Template, Template, bool, bool)>,
    pools: std::collections::HashSet<String>,
    generics: Vec<String>,
}

pub static mut TYPES: SyncLazy<Mutex<HashMap<TypeName, Contents>>> = SyncLazy::new(|| {
    Mutex::new(HashMap::new())
});

pub static mut POOLS: SyncLazy<Mutex<HashMap<TypeName, Contents>>> = SyncLazy::new(|| {
    Mutex::new(HashMap::new())
});

pub fn derive_poolcbindgen(input: TokenStream) -> TokenStream {
    // Parse the input tokens into a syntax tree.
    let input = parse_macro_input!(input as DeriveInput);
    let mods = crate::list(&input.attrs, "mods");
    let flags = open_flags(&input.attrs);
    let types = types(&input.attrs);

    // Used in the quasi-quotation below as `#name`.
    // let name = input.ident;

    let mut expanded = vec![];
    for m in &mods {

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
        let named_open = format_ident!("{}_named_open", name_str);
        let named_data_pointer = format_ident!("{}_named_data_pointer", name_str);
        let named_logged_pointer = format_ident!("{}_named_logged_pointer", name_str);
        let mod_name = format_ident!("__{}", name_str);
        let root_name = format_ident!("__{}_root_t", name_str);
        // let pool_mod = format_ident!("__{}_module", name_str);
        
        let mut all_pools = unsafe { match POOLS.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner()
        } };
        let entry = all_pools.entry(name_str.clone()).or_insert(Contents::default());

        expanded.push(quote! {
            corundum::pool!(#m);

            pub mod #mod_name {
                use super::*;
                use corundum::stm::{Logger, Notifier};
                use corundum::stl::HashMap as PHashMap;
                use corundum::gen::ByteObject;
                use std::collections::hash_map::DefaultHasher;
                use std::hash::{Hash, Hasher};
                use core::ffi::c_void;
                use std::os::raw::c_char;
                use std::ffi::CStr;
                use super::#m::*;

                #[allow(non_camel_case_types)]
                type #m = super::#m::Allocator;

                pub enum Container {
                    #types
                    Custom(Named),
                    None
                }

                impl Default for Container {
                    fn default() -> Self {
                        Container::None
                    }
                }

                pub struct #root_name {
                    pub(crate) objs: PMutex<PHashMap<u64, Container, Allocator>>
                }

                impl RootObj<Allocator> for #root_name {
                    fn init(j: &Journal) -> Self {
                        Self {
                            objs: PMutex::new(PHashMap::new(j))
                        }
                    }
                }

                #[no_mangle]
                pub extern "C" fn #fn_open(path: *const c_char, mut flags: u32) -> *const #root_name {
                    let path = unsafe { CStr::from_ptr(path).to_str().unwrap() };
                    if flags == 0 { flags = #flags; }
                    let res = Allocator::open::<#root_name>(path, flags).unwrap();
                    let p = &*res as *const #root_name;
                    std::mem::forget(res); // Keep the pool open
                    p
                }

                #[no_mangle]
                pub extern "C" fn #fn_close() -> bool {
                    if Allocator::is_open() {
                        unsafe { Allocator::close().unwrap(); }
                        true
                    } else {
                        false
                    }
                }

                #[no_mangle]
                pub extern "C" fn #fn_base() -> usize {
                    Allocator::start() as usize
                }

                #[no_mangle]
                pub extern "C" fn #fn_alloc(size: usize) -> *mut c_void {
                    let j = unsafe {
                        Journal::current(false)
                        .expect(&format!("{} cannot be used outside a transaction", stringify!(#fn_alloc)))
                    };
                    unsafe { Allocator::new_uninit_for_layout(size, &*j.0) as *mut c_void }
                }

                #[no_mangle]
                pub extern "C" fn #fn_dealloc(ptr: *mut c_void, size: usize) {
                    assert!(
                        Journal::is_running(),
                        "{} cannot be used outside a transaction",
                        stringify!(#fn_dealloc)
                    );
                    unsafe { Allocator::free_slice(std::slice::from_raw_parts_mut(ptr, size)); }
                }

                #[no_mangle]
                pub extern "C" fn #fn_allocated(off: u64, size: usize) -> bool {
                    if Allocator::allocated(off, size) || off == 0 {
                        true
                    } else {
                        eprintln!("off {} (len: {}) is not allocated", off, size);
                        false
                    }
                }
                #[no_mangle]
                pub extern "C" fn #fn_valid(ptr: *const c_void) -> bool {
                    Allocator::valid(ptr)
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
                        Allocator::commit();
                    }
                }

                #[no_mangle]
                pub extern "C" fn #fn_txn_rollback() {
                    unsafe {
                        corundum::ll::sfence();
                        if Allocator::rollback() {
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
                        if Allocator::valid(obj) {
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
                    Allocator::print_info()
                }

                #[no_mangle]
                pub extern "C" fn #fn_read64(addr: u64) -> u64 {
                    unsafe { *Allocator::get_unchecked(addr) }
                }

                pub struct Named(u8, ByteObject<Allocator>);

                #[no_mangle]
                pub extern "C" fn #named_open(p: &#root_name, name: *const c_char, size: usize, init: extern fn(*mut c_void)->()) -> *const c_void /* Named */ {
                    let name = unsafe { CStr::from_ptr(name).to_str().unwrap() };
                    let mut hasher = DefaultHasher::new();
                    name.hash(&mut hasher);
                    let key = hasher.finish();

                    let mut res: *const Named = std::ptr::null();
                    if transaction(AssertTxInSafe(|j| {
                        let mut objs = p.objs.lock(j);
                        if let Container::Custom(named) = objs.get_or_insert(key, || {
                            let mut obj = ByteObject::new_uninit(size, j);
                            init(unsafe { obj.as_ptr_mut() });
                            Container::Custom(Named(0, obj))
                        }, j) {
                            res = named as *const Named;
                        }
                    })).is_err() {
                        res = std::ptr::null_mut();
                    }
                    res as *const c_void
                }

                #[no_mangle]
                pub extern "C" fn #named_data_pointer(obj: *const c_void /* &Named */) -> *const c_void {
                    let obj = unsafe { corundum::utils::read::<Named>(obj as *mut u8) };
                    obj.1.as_ptr()
                }

                #[no_mangle]
                pub extern "C" fn #named_logged_pointer(obj: *mut c_void /* &Named */) -> *mut c_void {
                    let obj = unsafe { corundum::utils::read::<Named>(obj as *mut u8) };
                    &mut obj.0 as *mut u8 as *mut c_void
                }
            }
            #[allow(non_camel_case_types)]
            pub type #root_name = #mod_name::#root_name;
        });



        let contents = format!(
        "// This file is auto-generated by Corundum. Don't manually modify it.
#pragma once

#include <pstdlib>
#include <functional>
#include <stdio.h>
#include <execinfo.h>
#include <signal.h>
#include <stdlib.h>
#include <unistd.h>
#include <proot.h>

// forward declarations

template<>
struct pool_traits<{pool}> {{
    static size_t base;
    typedef struct{{}} journal;
    using handle = {root_name};
    using void_pointer = corundum::pointer_t<void, {pool}>;

    static void_pointer allocate(size_t size) {{
        auto res = {pool_alloc}(size);
        return void_pointer::from_unsafe(res);
    }}
    static void deallocate(void_pointer ptr, size_t __n) {{
        {pool_dealloc}(ptr.get(), __n);
    }}
    static bool allocated(size_t off, size_t __n) {{
        return {pool_allocated}(off, __n);
    }}
    static bool valid(const void* ptr) {{
        return {pool_valid}(ptr);
    }}
    static void print_info() {{
        {pool_print_info}();
    }}
    static void cell_log(const void *obj, const u_int8_t *logged, size_t size, const journal *j) {{
        {pool_log}(obj, logged, size, j);
    }}
    static const journal* journal_handle() {{
        return (const journal*) {pool_journal}();
    }}
    static const void *named_open(const {root_name} *p, const char *name, size_t size, void (*init)(void*)) {{
        return {pool_named_open}(p, name, size, init);
    }}
    static const void *named_data_pointer(const void *obj) {{
        return {pool_named_data_pointer}(obj);
    }}
    static void *named_logged_pointer(void *obj) {{
        return {pool_named_logged_pointer}(obj);
    }}
}};

size_t pool_traits<{pool}>::base = 0;
class {pool}: public corundum::pool_type {{
    const {root_name} *inner;
public:
    typedef pool_traits<{pool}>::journal journal;
    // type aliases
    template<class T> using root = proot_t<T, {pool}>;
    template<class T> using make_persistent = corundum::make_persistent<T, {pool}>;
    template<class T> using cell = corundum::cell<T, {pool}>;
    {pool}(const char* path, u_int32_t flags, bool check_open = true) {{
        if (check_open) assert(pool_traits<{pool}>::base==0, \"{pool} was already open\");
        inner = {pool_open}(path, flags);
        pool_traits<{pool}>::base = {pool_base}();
    }}
    ~{pool}() {{
        {pool_close}();
        pool_traits<{pool}>::base = 0;
    }}
    const {root_name} *handle() const {{
        return inner;
    }}
    static bool txn(std::function<void(const journal*)> f) {{
        auto j = {pool_txn_begin}();
        try {{
            f((const journal*)j);
        {pool_txn_commit}();
            return true;
        }} catch (const std::exception& ex) {{
            std::cerr << \"runtime error: \" << ex.what() << std::endl;
        }} catch (const std::string& ex) {{
            std::cerr << \"runtime error: \" << ex << std::endl;
        }} catch (const char *e) {{
            std::cerr << \"runtime error: \" << e << std::endl;
        }} catch (...) {{
            std::cerr << \"runtime error: unsuccessful transaction\" << std::endl;
        }}
        {pool_txn_rollback}();
        return false;
    }}
}};",
pool = m,
pool_alloc = fn_alloc.to_string(),
pool_allocated = fn_allocated.to_string(),
pool_dealloc = fn_dealloc.to_string(),
pool_valid = fn_valid.to_string(),
pool_print_info = fn_print_info.to_string(),
pool_log = fn_log.to_string(),
pool_journal = fn_journal.to_string(),
pool_open = fn_open.to_string(),
pool_close = fn_close.to_string(),
pool_base = fn_base.to_string(),
pool_txn_begin = fn_txn_begin.to_string(),
pool_txn_commit = fn_txn_commit.to_string(),
pool_txn_rollback = fn_txn_rollback.to_string(),
pool_named_open = named_open.to_string(),
pool_named_data_pointer = named_data_pointer.to_string(),
pool_named_logged_pointer = named_logged_pointer.to_string(),
root_name = root_name.to_string(),
);
        entry.contents = contents;

        // if let Ok(mut file) = std::fs::File::create(format!("inc/{}.hpp", name_str)) {
        //     let _=file.write_all(export.as_bytes());
        // }
    }

    let expanded = quote! { #(#expanded)* };

    // Hand the output tokens back to the compiler.
    TokenStream::from(expanded)
}

pub fn derive_cbindgen(input: TokenStream) -> TokenStream {
    // Parse the input tokens into a syntax tree.
    let input = parse_macro_input!(input as DeriveInput);

    let mut found_pool_generic: Option<Ident> = None;
    let mut ogen = vec!();
    if !input.generics.params.is_empty() {
        for t in &input.generics.params {
            if let GenericParam::Type(t) = t {
                let mut is_pool = false;
                for b in &t.bounds {
                    if let TypeParamBound::Trait(b) = b {
                        if let Some(p) = b.path.get_ident() {
                            if p == "MemPool" {
                                if let Some(other) = found_pool_generic {
                                    abort!(t.span(),
                                        "multiple generic parameters are assigned as memory pool";
                                        note = "previous MemPool parameter is {}", other
                                    );
                                }
                                found_pool_generic = Some(t.ident.clone());
                                is_pool = true;
                            }
                        }
                    }
                }
                if !is_pool {
                    ogen.push(t.ident.clone());
                }
            } else if let GenericParam::Const(_) = t {
                abort!(t.span(),
                    "const type parameters are not FFI-compatible";
                    help = "you may want to use type aliasing to statically assign a value to it"
                );
            }
         }
    }
    if found_pool_generic.is_none() {
        if let Some(w) = &input.generics.where_clause {
            for w in &w.predicates {
                if let WherePredicate::Type(t) = w {
                    for b in &t.bounds {
                        if let TypeParamBound::Trait(b) = b {
                            if let Some(p) = b.path.get_ident() {
                                if p == "MemPool" {
                                    if let Some(other) = found_pool_generic {
                                        abort!(t.span(),
                                            "multiple generic parameters are assigned as memory pool";
                                            note = "previous MemPool parameter is {}", other
                                        );
                                    }
                                    if let Type::Path(p) = &t.bounded_ty {
                                        if p.path.segments.len() == 1 {
                                            found_pool_generic = Some(p.path.segments[0].ident.clone());
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    if !ogen.is_empty() {
        let pool = if let Some(p) = &found_pool_generic { p.to_string() } else { "".to_owned() };
        let mut abort = false;
        for t in &ogen {
            if *t != pool {
                emit_warning!(t.span(),
                    "FFI-incompatible generic type parameter";
                    help = "add {} to the generics list and remove it from here; use corundum::gen::ByteObject instead", t
                );
                abort = true;
            }
        }
        if abort {
            abort!(input.ident.span(),
                "struct {} should have exactly one generic type parameter implementing MemPool trait", input.ident;
                help = "use corundum::gen::ByteObject instead of the generic types, and specify the generic types using `generics(...)` attribute (e.g., #[generics({})])", 
                ogen.iter().map(|v| v.to_string()).collect::<Vec<String>>().join(", ")
            )
        }
    }
    if found_pool_generic.is_none() {
        abort!(input.ident.span(),
            "struct {} should be generic with regard to the pool type", input.ident;
            help = "specify a generic type which implements MemPool trait"
        )
    }

    let mods = crate::list(&input.attrs, "mods");
    let generics = crate::list(&input.attrs, "generics");

    // let mut all_pools = unsafe { match POOLS.lock() {
    //     Ok(g) => g,
    //     Err(p) => p.into_inner()
    // } };

    // for m in &mods {
    //     let entry = all_pools.entry(m.to_string()).or_insert(Contents::default());
    //     entry.pools.push(input.ident.to_string());
    // }

    if generics.is_empty() {
        abort!(input.ident.span(),
            "struct {} should have at least one generic type parameter as the data type", input.ident;
            help = "specify the generic data types using `generics(...)` attribute (e.g., #[generics(T)])"
        )
    }

    let generics_list = quote!{ #(class #generics,)*; }.to_string();
    let generics_list = generics_list.replace(", ;", "");
    let generics_str = quote!{ #(#generics,)*; }.to_string().replace(", ;", "");

    // Used in the quasi-quotation below as `#name`.
    let name = input.ident;
    let name_str = name.to_string();
    let small_name = name_str.to_lowercase();
    let cname = format!("p{}_t", small_name);

    let mut expanded = vec![];
    let mut includes = "".to_owned();
    let mut all_types = unsafe { match TYPES.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner()
    } };
    let mut entry = all_types.entry(name_str.clone()).or_insert(Contents::default());
    entry.generics = generics.iter().map(|v| v.to_string()).collect();
    for m in &mods {

        // // Generate an expression to sum up the heap size of each field.
        // let sum = root_all_fields(&name, &input.data);

        let pool = m.to_string();
        let __m = format_ident!("__{}_root_t", pool);
        let __mod = format_ident!("__{}", pool);
        let mod_demangled = pool.replace("::", "_").replace("<", "_").replace(">", "_").replace(" ", "").to_lowercase();
        let name_str = format!("__{}_{}", mod_demangled, name_str.to_lowercase());
        let fn_new = format_ident!("{}_new", name_str);
        let fn_drop = format_ident!("{}_drop", name_str);
        let fn_open = format_ident!("{}_open", name_str); 
        let mod_name = format_ident!("{}_{}", name_str, pool);


        expanded.push(quote! {
            pub mod #mod_name {
                use super::*;
                use std::ffi::c_void;
                use std::os::raw::c_char;
                use std::ffi::CStr;
                use std::collections::hash_map::DefaultHasher;
                use std::hash::{Hash, Hasher};

                #[allow(non_camel_case_types)]
                type #m = super::#m::Allocator;

                #[no_mangle]
                pub extern "C" fn #fn_new(j: *const c_void) -> *const #name<#m> {
                    use corundum::boxed::Pbox;

                    assert!(!j.is_null(), "transactional operation outside a transaction");
                    unsafe {
                        let j = corundum::utils::read::<corundum::stm::Journal<#m>>(j as *mut u8);
                        Pbox::leak(Pbox::new(#name::new(), j)) as *mut #name<#m>
                    }
                }

                #[no_mangle]
                pub extern "C" fn #fn_drop(obj: *mut #name<#m>) {
                    use corundum::boxed::Pbox;

                    assert!(!obj.is_null(),);
                    unsafe {
                        Pbox::<#name<#m>,#m>::from_raw(obj); // drops when out of scope
                    }
                }

                #[no_mangle]
                pub extern "C" fn #fn_open(p: &#__m, name: *const c_char) -> *const #name<#m> {
                    let name = unsafe { CStr::from_ptr(name).to_str().unwrap() };
                    let mut hasher = DefaultHasher::new();
                    name.hash(&mut hasher);
                    let key = hasher.finish();

                    let mut res: *const #name<#m> = std::ptr::null();
                    if transaction(AssertTxInSafe(|j| {
                        let mut objs = p.objs.lock(j);
                        let obj = objs.get_or_insert(key, || {
                            #__mod::Container::#name(#name::new())
                        }, j);
                        if let #__mod::Container::#name(obj) = &obj {
                            res = obj as *const #name<#m>;
                        }
                    })).is_err() {
                        res = std::ptr::null();
                    }
                    res
                }
            }
        });

        includes += &format!("\n#include \"{pool}.hpp\"", pool = pool);

        entry.pools.insert(pool.clone());
        entry.traits.insert(pool.clone(), format!("
template<>
struct {small_name}_traits<{pool}> {{
    typedef typename pool_traits<{pool}>::journal journal;
    static const {name}<{pool}>* create(const journal *j) {{
        return {fn_new}(j);
    }}
    static void drop({name}<{pool}> *obj) {{
        {fn_drop}(obj);
    }}
    static const {name}<{pool}>* open(const {root_name} *p, const char *name) {{
        return {fn_open}(p, name);
    }}
    // specialized methods
}};\n",
small_name = small_name,
name = name,
pool = pool,
fn_new = fn_new.to_string(),
fn_open = fn_open.to_string(),
fn_drop = fn_drop.to_string(),
root_name = __m.to_string()
        ));
    }

    entry.contents = format!(
        "// This file is auto-generated by Corundum. Do not modify.
#pragma once

#include <rstl.h>
#include <assert.h>
#include <iostream>
#include <corundum>
#include <unordered_set>
#include <pstdlib>
#include <cstring>
{includes}

template<class _P>
using pstring = typename corundum::make_persistent<std::string, _P>::type;

template<class _P>
struct {small_name}_traits {{
    typedef typename pool_traits<_P>::journal journal;
    static const {name}<_P>* create(const journal *j);
    static void drop({name}<_P> *obj);
    static const {name}<_P>* open(const void *p, const char *name);
    // template methods
}};

template <{generics_list}, class _P>
class {cname} : public corundum::psafe_type_parameters {{ 

    typedef pool_traits<_P>                        pool_traits;
    typedef typename pool_traits::handle          handle;
    typedef typename pool_traits::journal         journal;
    typedef corundum::pointer_t<{name}<_P>, _P>   pointer;

    pointer inner;
    pstring<_P> name;
    bool is_root;

    static std::unordered_set<std::string> objs;

private:
    inline const {name}<_P>* self() const {{
        return inner.operator->();
    }}

public:
    {cname}(const journal *j, const std::string &name = \"(anonymous)\") {{ 
        inner = pointer::from({small_name}_traits<_P>::create(j));
        this->name = name.c_str();
        is_root = false;
    }} 

    {cname}(const {cname} &stk) {{ 
        inner = stk.inner;
        name = stk.name;
        is_root = false;
    }} 

    {cname}(const handle *pool, const std::string &name) noexcept(false) {{ 
        _P::txn([this,&pool,&name](auto j) {{ 
            assert(objs.find(name) == objs.end(), \"'%s' was already open\", name.c_str());
            this->name = name.c_str();
            objs.insert(name);
            pointer stk = pointer::from_unsafe({small_name}_traits<_P>::open(pool, name.c_str()));
            memcpy((void*)&inner, (void*)&stk, sizeof(pointer));
            is_root = true;
        }} );
    }} 

    ~{cname}() noexcept(false) {{ 
        if (is_root) {{ 
            auto n = name.c_str();
            assert(objs.find(n) != objs.end(), \"'%s' is not open\", n);
            objs.erase(n);
        }}  else {{ 
            {small_name}_traits<_P>::drop(
                static_cast<{name}<_P>*>(
                    static_cast<void*>(inner)
                )
            );
        }} 
    }} 

    // other methods
}};

template <{generics_list}, class _P> std::unordered_set<std::string> {cname}<{generics}, _P>::objs;
",
generics_list = generics_list,
generics = generics_str,
includes = includes,
small_name = small_name,
name = name_str,
cname = cname
);
    entry.decl = format!("template<{generics_list}, class _P> class p{name}_t;",
            name = small_name,
            generics_list = generics_list
        );
    entry.alias = format!("template<{generics_list}> using {name} = p{name}_t<{generics}, {{pool}}>;",
            name = small_name,
            generics = generics_str,
            generics_list = generics_list
        );
    

    let expanded = quote! { #(#expanded)* };

    // Hand the output tokens back to the compiler.
    TokenStream::from(expanded)
}

fn types(attrs: &Vec<Attribute>) -> TokenStream2 {
    let types = crate::list(attrs, "types");
    let recurse = types.iter().map(|name| {
        let name_str = name.to_string().replace(" ", "");
        let parts: Vec<&str> = name_str.split("::").collect();
        let ident = format_ident!("{}", parts.last().unwrap());
        quote_spanned!(name.span()=> #ident(super::#name<Allocator>))
    });
    quote! {
        #(#recurse,)* 
    }
}

fn open_flags(attrs: &Vec<Attribute>) -> TokenStream2 {
    let mut vflags = vec![];
    for attr in attrs {
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

fn refine_path(m: &TokenStream2, p: &mut Path, tmpl: &Vec<String>, ty_tmpl: &Vec<String>, gen: &Vec<String>, check: i32, modify: bool, has_generics: &mut Option<&mut bool>) {
    for s in &mut p.segments {
        match &mut s.arguments {
            PathArguments::AngleBracketed(args) => {
                for g in &mut args.args {
                    match g {
                        GenericArgument::Type(ty) => { check_generics(m, ty, &tmpl, ty_tmpl, gen, if s.ident != "Gen" && check == 1 { 1 } else { 0 }, modify, has_generics); }
                        GenericArgument::Binding(b) => { check_generics(m, &mut b.ty, &tmpl, ty_tmpl, gen, check, modify, has_generics); }
                        _ => ()
                    }
                }
            }
            PathArguments::Parenthesized(args) => {
                for i in &mut args.inputs { check_generics(m, i, tmpl, ty_tmpl, gen, check, modify, has_generics); }
                if let ReturnType::Type(_, ty) = &mut args.output {
                    check_generics(m, ty, tmpl, ty_tmpl, gen, check, modify, has_generics);
                }
            }
            _ => ()
        }
    }
}

fn check_generics(m: &TokenStream2, ty: &mut Type, tmpl: &Vec<String>, ty_tmpl: &Vec<String>, gen: &Vec<String>, check: i32, modify: bool, has_generics: &mut Option<&mut bool>) -> bool {
    let res = match ty {
        Type::Array(a) => check_generics(m, &mut *a.elem, tmpl, ty_tmpl, gen, check, modify, has_generics),
        Type::BareFn(f) => {
            for i in &mut f.inputs { 
                if check_generics(m, &mut i.ty, tmpl, ty_tmpl, gen, 2, modify, has_generics) {
                    abort!(
                        i.ty.span(), "no bindings found for template parameters";
                        note = "use template types in form of references or pointers"
                    );
                } 
            }
            if let ReturnType::Type(_, ty) = &mut f.output {
                check_generics(m, ty, tmpl, ty_tmpl, gen, 2, modify, has_generics);
            }
            false
        },
        Type::Group(g) => check_generics(m, &mut *g.elem, tmpl, ty_tmpl, gen, check, modify, has_generics),
        Type::Paren(ty) => check_generics(m, &mut *ty.elem, tmpl, ty_tmpl, gen, check, modify, has_generics),
        Type::Path(p) => {
            if p.path.segments.len() == 1 {
                if p.path.segments[0].arguments == PathArguments::None {
                    let name = p.path.segments[0].ident.to_string();
                    if tmpl.contains(&name) {
                        if !gen.contains(&name) {
                            abort!(
                                ty.span(), "template parameter `{}` is not in the generic type list", &name;
                                note = "consider adding `{}` to the `generics()` attribute of the implementing type, or choose from {}", &name, gen.join(", ")
                            );
                        }
                        if modify {
                            *ty = parse2(quote!(std::ffi::c_void)).unwrap();
                        }
                        if let Some(has_generics) = has_generics {
                            **has_generics = true;
                        }
                        true
                    } else if ty_tmpl.contains(&name) {
                        if modify {
                            *ty = parse2(quote!(#m)).unwrap();
                        }
                        true
                    } else {
                        false
                    }
                } else {
                    refine_path(m, &mut p.path, tmpl, ty_tmpl, gen, check, modify, has_generics);
                    false
                }
            } else {
                refine_path(m, &mut p.path, tmpl, ty_tmpl, gen, check, modify, has_generics);
                false
            }
        }
        // tmpl.contains(&p.path.get_ident().unwrap().ident.to_string()),
        Type::Ptr(p) => {
            if check_generics(m, &mut *p.elem, tmpl, ty_tmpl, gen, if check == 2 { 0 } else { check }, modify, has_generics) {
                // update(ty);
                // *ty = parse2(quote!(corundum::gen::Gen)).unwrap();
                // if modify {
                //     // *ty = parse2(quote!(corundum::gen::Gen)).unwrap();
                //     *p.elem = parse2(quote!(std::ffi::c_void)).unwrap();
                // }
            }
            false
        },
        Type::Reference(r) =>  {
            if check_generics(m, &mut *r.elem, tmpl, ty_tmpl, gen, if check == 2 { 0 } else { check }, modify, has_generics) {
                // update(ty);
                // *ty = parse2(quote!(corundum::gen::Gen)).unwrap();
                // if modify {
                //     // *ty = parse2(quote!(corundum::gen::Gen)).unwrap();
                //     *r.elem = parse2(quote!(std::ffi::c_void)).unwrap();
                // }
            } 
            false
        },
        Type::Slice(s) => check_generics(m, &mut *s.elem, tmpl, ty_tmpl, gen, check, modify, has_generics),
        Type::Tuple(t) => t.elems.iter_mut().any(|t| check_generics(m, t, tmpl, ty_tmpl, gen, check, modify, has_generics)),
        Type::Verbatim(v) => {
            let name = v.to_string();
            if tmpl.contains(&name) {
                // if modify {
                //     *ty = parse2(quote!(std::ffi::c_void)).unwrap();
                // }
                if let Some(has_generics) = has_generics {
                    **has_generics = true;
                }
                true
            } else if ty_tmpl.contains(&name) {
                if modify {
                    *ty = parse2(quote!(#m)).unwrap();
                }
                true
            } else {
                false
            }
        },
        _ => false
    };
    if res && check == 1 {
        let msg = if let Some(p) = ty_tmpl.first() {
            format!("consider using corundum::gen::Gen<{}, {}>", quote!(#ty), p.clone())
        } else {
            "consider using corundum::gen::Gen".to_owned()
        };
        abort!(
            ty.span(), "no bindings found for template parameters";
            note = "{}", msg
        );
    }
    res
}

pub fn cbindgen(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let mut expanded = vec![];
    let mut extern_mod = format_ident!("__extern_mod_0");
    if let Ok(imp) = parse2::<ItemImpl>(item.clone().into()) {
        extern_mod = format_ident!("__extern_mod_{}_{}", imp.span().start().line, imp.span().start().column);
        if let Type::Path(ref tp) = *imp.self_ty {
            let ty_gen: Template = imp.generics.params.iter().map_while(|t| 
                if let GenericParam::Type(t) = t {
                    if t.bounds.iter().any(|b| if let TypeParamBound::Trait(t) = b {
                            t.path.get_ident().unwrap() == "MemPool"
                        } else {
                            false
                        }
                    ) {
                        Some(t.ident.to_string())
                    } else {
                        if let Some(w) = &imp.generics.where_clause {
                            let mut res = None;
                            for p in &w.predicates {
                                if let WherePredicate::Type(t) = p {
                                    for b in &t.bounds {
                                        if let TypeParamBound::Trait(tr) = b {
                                            if tr.path.get_ident().unwrap() == "MemPool" {
                                                if let Type::Path(p) = &t.bounded_ty {
                                                    res = Some(p.path.get_ident().unwrap().to_string());
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            res
                        } else {
                            None
                        }
                    }
                } else {
                    None
                }
            ).collect();
            let name = &tp.path.segments.last().unwrap().ident;
            let small_name = name.to_string().to_lowercase();
    
            let mut types = unsafe { match TYPES.lock() {
                Ok(g) => g,
                Err(p) => p.into_inner()
            }};
    
            let entry = types.entry(name.to_string()).or_insert(Contents::default());
            
            for pool in &entry.pools {
                let pool = format_ident!("{}", pool);
                expanded.push(quote! {
                    #[allow(non_camel_case_types)]
                    type #pool = crate::#pool::Allocator;
                });
            }

            for item in imp.items {
                if let ImplItem::Method(func) = item {

                    let mut spc = func.clone();
                    let mut inputs = Punctuated::<_, Token![,]>::new();
                    let mut args = vec!();
                    let gen: Template = spc.sig.generics.params.iter()
                        .map_while(|i| 
                            if let GenericParam::Type(t) = i {
                                Some(t.ident.to_string())
                            } else {
                                None
                            }
                        ).collect();
                    {
                        let mut i = spc.sig.inputs.iter_mut();
                        if i.next().is_some() {
                            while let Some(a) = i.next() {
                                if let FnArg::Typed(PatType { pat, ty, .. }) = a {
                                    if let Pat::Ident(PatIdent { ident, .. }) = &**pat {
                                        let mut has_generics = false;
                                        check_generics(&quote!(), &mut *ty, &gen, &ty_gen, &entry.generics, 1, false, &mut Some(&mut has_generics));
                                        // eprintln!("fn {}: {}", spc.sig.ident, quote!(#ty));
                                        args.push((has_generics, ident.to_string()));
                                    }
                                }
                                inputs.push(a.clone());
                            } 
                        }
                    }
                    spc.sig.generics = Generics::default();
                    spc.sig.inputs = inputs;
                    let mut output_has_generics = false;
                    if let ReturnType::Type(_, ty) = &mut spc.sig.output {
                        check_generics(&quote!(), ty, &gen, &ty_gen, &entry.generics, 1, false, &mut Some(&mut output_has_generics));
                    }
                    if let Ok(abi) = parse2::<Abi>(quote!(extern "C")) {
                        spc.sig.abi = Some(abi);
                    }
                    spc.block = parse2(quote!{{
                        () // no implementation
                    }}).unwrap();
                    entry.funcs.push((
                        spc.sig.ident.to_string(), 
                        args, quote!(#[no_mangle] #spc).to_string(), 
                        gen.clone(), 
                        ty_gen.clone(), 
                        spc.sig.output != ReturnType::Default, 
                        output_has_generics));

                    for m in &entry.pools {
                        let m = format_ident!("{}", m);
                        let m_str = m.to_string();
                        let mut tps = vec![];
                        let mut ext = func.clone();
                        for param in &func.sig.generics.params {
                            if let GenericParam::Type(ty) = param {
                                tps.push(ty.ident.clone());
                            }
                        }
                        ext.sig.ident = format_ident!("__{}_{}_{}", m_str, small_name, ext.sig.ident);
                        if let Ok(abi) = parse2::<Abi>(quote!(extern "C")) {
                            ext.sig.abi = Some(abi);
                        }
                        ext.sig.generics = Generics::default();
                        let mut has_receiver = false;
                        if let Some(first) = ext.sig.inputs.first_mut() {
                            if let FnArg::Receiver(_) = first { has_receiver = true; }
                            if has_receiver {
                                if let Ok(arg) = parse2::<FnArg>(quote!(__self: &#name<#m>)) {
                                    *first = arg;
                                }
                            }
                        }
                        if has_receiver {
                            let fname = func.sig.ident.clone();
                            let mut args = vec![];
                            for arg in &mut ext.sig.inputs {
                                if let FnArg::Typed(PatType { pat, ty, .. }) = arg {
                                    if let Pat::Ident(PatIdent { ident, .. }) = &mut **pat {
                                        if ident != "__self" {
                                            check_generics(&quote!(#m), ty, &gen, &ty_gen, &entry.generics, 1, true, &mut None);
                                            args.push(quote!(#ident));
                                        }
                                    }
                                }
                            }
                            if let ReturnType::Type(_, ty) = &mut ext.sig.output {
                                check_generics(&quote!(#m), ty, &gen, &ty_gen, &entry.generics, 1, true, &mut None);
                            }
                            ext.block = parse2(quote!{{
                                __self.#fname(#(#args,)*)
                            }}).unwrap();

                            expanded.push(quote!{
                                #[no_mangle]
                                #[deny(improper_ctypes_definitions)] 
                                #ext
                            });
                        }
                    }
                }
            }
        }
    }
    let item: TokenStream2 = item.into();
    let expanded = quote! { 
        #item
        mod #extern_mod {
            use super::*;
            #(#expanded)*
        }
    };

    // Hand the output tokens back to the compiler.
    TokenStream::from(expanded)
    // item
}

fn parse_c_fn(sig: &str, name: &str) -> (String, Vec<String>) {
    fn is_word(c: char) -> bool {
        (c >= 'a' && c <= 'z') ||
        (c >= 'A' && c <= 'Z') ||
        (c >= '0' && c <= '9') ||
        c == '_'
    }

    let mut par = 0; // (...)
    let mut br = 0;  // [...]
    let mut lt = 0;  // <...>
    let mut args = vec!();
    let mut ret = String::new();
    let mut token = String::new();
    let mut tokens = vec!();
    for c in sig.chars() {
        if !is_word(c) {
            if !token.is_empty() {
                tokens.push(token.clone());
            }
            token.clear();
            if c != ' ' {
                tokens.push(String::from(c));
                continue;
            }
        }
        if c != ' ' {
            token.push(c);
        }
    }
    if !tokens.is_empty() {
        tokens.push(token.clone());
    }
    token.clear();
    let mut args_began = false;
    for t in tokens {
        if t == name && !args_began {
            ret = token.clone();
            token.clear();
            args_began = true;
        } else {
            match t.as_str() {
                "(" => {
                    par += 1;
                    if par == 1 { continue; }
                },
                ")" => {
                    if par == 1 && br == 0 && lt == 0 {
                        args.push(token.clone());
                        token.clear();
                        continue;
                    }
                    par -= 1
                },
                "[" => br += 1,
                "]" => br -= 1,
                "<" => lt += 1,
                ">" => lt -= 1,
                "," => {
                    if par == 1 && br == 0 && lt == 0 {
                        args.push(token.clone());
                        token.clear();
                        continue;
                    }
                }
                _ => ()
            }
            token += &(t + " ");
        }
    }
    (ret, args)
}

use std::path::PathBuf;

pub fn export(dir: PathBuf, span: proc_macro2::Span, overwrite: bool, warning: bool) -> std::io::Result<()> {
    if let Ok(mut iter) = dir.read_dir() {
        if iter.next().is_some() {
            if overwrite {
                if warning {
                    emit_warning!(
                        span, "directory {} is not empty", dir.to_string_lossy();
                        note = "overwriting files"
                    );
                }
            } else {
                abort!(
                    span, "directory {} is not empty", dir.to_string_lossy();
                    note = "aborting export procedure";
                    help = "use 'overwrite=true' to force generating header files"
                );
            }
        }
    } else {
        if warning {
            emit_warning!(
                span, "directory {} does not exist", dir.to_string_lossy();
                note = "creating directory {}", dir.to_string_lossy();
            );
        }
    }

    create_dir_all(&dir)?;
    let mut pools = unsafe { match POOLS.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner()
    } };
    let mut types = unsafe { match TYPES.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner()
    } };

    for (ty, cnt) in &mut *types {
        let alias = cnt.alias.clone();
        let fwd_decl = cnt.decl.clone();
        for p in &cnt.pools {
            if let Some(pool) = pools.get_mut(&*p) {
                let alias = alias.replace("{pool}", p);
                pool.contents = pool.contents.replace("// forward declarations",
                    &format!("// forward declarations\n{}", fwd_decl));
                pool.contents = pool.contents.replace("    // type aliases",
                    &format!("    // type aliases\n    {}", alias));
            }
        }
        let mut cbindfile = "".to_owned();
        // let mut funcs = vec!();
        for (_, _, f, _, _, _, _) in &mut cnt.funcs {
            let re = Regex::new(&format!(r"\#\[no_mangle\].*")).unwrap();
            if re.find(f).is_some() {
                let re = Regex::new(&format!(r"\bGen\b\s*<\s*(\w+)\s*>")).unwrap();
                *f = re.replace_all(f, "&$1").to_string();
                cbindfile += &f;
                cbindfile += "\n";
            }
        }

        if let Ok(mut file) = File::create("/tmp/___corundum_tmp_file.rs") {
            use cbindgen::Builder;
            file.write_all(cbindfile.as_bytes())?;
            file.flush()?;
            drop(file);
            let builder = Builder::new();
            let bindings = match Builder::with_src(builder, "/tmp/___corundum_tmp_file.rs").generate() {
                Ok(bindings) => bindings,
                Err(msg) => {
                    if if let Ok(build) = std::env::var("CBINDGEN") {
                        build == "1"
                    } else { false } {
                        abort_call_site!("conversion failed: {}", msg)
                    } else {
                        emit_call_site_warning!("conversion failed: {}", msg);
                        return Ok(());
                    }
                }
            };
            // let mut stream = SourceWriter::new();
            bindings.write_to_file("/tmp/___corundum_tmp_file.h");
            let s = read_to_string("/tmp/___corundum_tmp_file.h")?;

            for (name, args, sig, tmp, ty_pool, has_return, _) in &mut cnt.funcs {
                let tmpl = if tmp.is_empty() { "".to_owned() } else {
                    format!("template<class {}> ", tmp.join(", class"))
                };
                let tmpl_kw = if tmp.is_empty() { "" } else { "template " }.to_owned();
                let gen = if tmp.is_empty() { "".to_owned() } else { 
                    format!("<{}>", tmp.join(","))
                }.to_owned();
                let args = args.iter().map(|(_, n)| n.to_owned()).collect::<Vec<String>>().join(", ");
                let re = Regex::new(&format!(r"(.+\W+{}\(.*\));", name)).unwrap();
                let re_pool = if ty_pool.is_empty() { None } else { 
                    Some(Regex::new(&format!(r"\b{}\b", ty_pool[0])).unwrap()) 
                };
                if let Some(cap) = re.captures(&s) {
                    *sig = cap.get(1).unwrap().as_str().to_owned();
                    if let Some(re) = re_pool {
                        *sig = re.replace_all(sig, "_P").to_string();
                    }
                    cnt.contents = cnt.contents.replace("    // template methods",
                        &format!("    // template methods\n    {}static {};",
                        tmpl,
                        sig.replacen(
                            &format!("{}(", name), 
                            &format!("{}(const {}<_P> *__self, ", name, ty), 1)
                            .replace(", )", ")")));
                    let ret_tok = if *has_return { "return " } else { "" };
                        cnt.contents = cnt.contents.replace("    // other methods",
                            &format!("    // other methods\n    {sig} {{
        {ret}{ty}_traits<_P>::{tmp}{fn}{gen}(self(){comma}{args});
    }}\n",
    ret = ret_tok,
    ty = ty.to_lowercase(),
    sig = sig,
    tmp = tmpl_kw,
    gen = gen,
    fn = name,
    comma = if args.is_empty() { "" } else { ", " },
    args = args
));
                }
                // else {
                //         emit_call_site_warning!(
                //             "FFI-incompatible function signature in {}::{}", ty, name;
                //             note = "aborting export procedure";
                //         );
                // }
            }
            
        }

        for (p, t) in &mut cnt.traits {
            for (f, args, sig, tmp, _, ret, ret_gen) in &mut cnt.funcs {
                
                let (cret, cargs) = parse_c_fn(sig, f);
                let re = Regex::new(r"\bGen\b").unwrap();
                let cast = if *ret_gen && re.find(&cret).is_none() {
                    format!("({})", cret)
                } else {
                    "".to_owned()
                };
                let mut arglist = vec!();
                for i in 0..args.len() {
                    let (gen, n) = &args[i];
                    arglist.push(if *gen {
                        if re.find(&cargs[i]).is_some() {
                            n.to_owned()
                        } else {
                            let mut res = format!("({}){}", cargs[i].replace(&format!(" {} ", n), " "), n);
                            for t in tmp.clone() {
                                res = res.replace(&format!(" {} ", t), " void ");
                            }
                            res
                        }
                    } else {
                        n.to_owned()
                    });
                }
                let tmp = if tmp.is_empty() { "".to_owned() } else {
                    format!("template<class {}> ", tmp.join(", class"))
                };
                let args = arglist.join(", ");
                let old_sig = sig.clone();
                let re = Regex::new(r"\b_P\b").unwrap();
                *sig = re.replace_all(sig, p as &str).to_string();
                *t = t.replace("    // specialized methods",
                    &format!("    // specialized methods\n    {}static {}{{\n        {}\n    }}",
                        tmp,
                        sig.replacen(
                            &format!("{}(", f), 
                            &format!("{}(const {}<{}> *__self, ", f, ty, p), 1)
                            .replace(", )", ")"),
                        &format!("{ret}{cast}__{pool}_{type}_{fn}(__self{comma}{args});",
                            ret = if *ret { "return " } else { "" },
                            pool = p,
                            type = ty.to_lowercase(),
                            fn = f,
                            cast = cast,
                            comma = if args.is_empty() { "" } else { ", " },
                            args = args)
                        ));
                *sig = old_sig;
            }
            cnt.contents += t;
        }
    }

    if let Ok(build) = std::env::var("CBINDGEN") {
        if build == "1" {
            for (ty, content) in &*types {
                let path = format!("{}/{}.hpp", dir.to_string_lossy(), ty.to_lowercase());
                if let Ok(mut file) = File::create(path) {
                    let _=file.write_all(content.contents.as_bytes());
                }
            }
            for (pool, content) in &*pools {
                let path = format!("{}/{}.hpp", dir.to_string_lossy(), pool);
                if let Ok(mut file) = File::create(path) {
                    let _=file.write_all(content.contents.as_bytes());
                }
            }
        }
    }

    Ok(())
}