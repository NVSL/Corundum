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
    funcs: Vec<(FuncName, FuncArgs, FuncSig, Template, TypeName, bool, bool, bool, bool)>,
    pools: std::collections::HashSet<String>,
    generics: Vec<String>,
    attrs: Attributes
}

#[derive(Default)]
pub struct Attributes {
    concurrent: bool
}

pub static mut TYPES: SyncLazy<Mutex<HashMap<TypeName, Contents>>> = SyncLazy::new(|| {
    Mutex::new(HashMap::new())
});

pub static mut POOLS: SyncLazy<Mutex<HashMap<TypeName, Contents>>> = SyncLazy::new(|| {
    Mutex::new(HashMap::new())
});

fn check_type(ty: &Type, pool_type: &Ident, gen_idents: &Vec<Ident>, warn: bool) {
    match ty {
        Type::Slice(s) => check_type(s.elem.as_ref(), pool_type, gen_idents, warn),
        Type::Array(a) => check_type(a.elem.as_ref(), pool_type, gen_idents, warn),
        Type::Tuple(t) => {
            for e in &t.elems {
                check_type(&e, pool_type, gen_idents, warn);
            }
        }
        Type::Path(p) => {
            if let Some(id) = p.path.get_ident() {
                if warn {
                    if gen_idents.contains(id) && id != pool_type {
                        emit_warning! {
                            id.span(), "direct use of template parameter is not safe";
                            help = "consider using corundum::gen::ByteArray<{}, {}>", id, pool_type;
                            help = "use `#[attrs(allow_generics)]` to disable this warning"
                        }
                    }
                }
            } else if let Some(last) = p.path.segments.last() {
                if last.ident == "ByteArray" {
                    if let PathArguments::AngleBracketed(args) = &last.arguments {
                        for arg in &args.args {
                            if let GenericArgument::Type(t) = arg {
                                if let Type::Path(p) = t {
                                    if let Some(id) = p.path.get_ident() {
                                        if !gen_idents.contains(id) {
                                            emit_error!(arg.span(), "not a generic parameter")
                                        } else {
                                            return;
                                        }
                                    } else {
                                        emit_error!(arg.span(), "not a generic parameter")
                                    }
                                } else {
                                    emit_error!(arg.span(), "not a generic parameter")
                                }
                            } else {
                                emit_error!(arg.span(), "not a generic parameter")
                            }
                        }
                    }
                }
            }
            for segment in &p.path.segments {
                if let PathArguments::AngleBracketed(args) = &segment.arguments {
                    for arg in &args.args {
                        if let GenericArgument::Type(t) = arg {
                            check_type(t, pool_type, gen_idents, warn);
                        }
                    }
                }
            }
        }
        Type::Paren(p) => check_type(p.elem.as_ref(), pool_type, gen_idents, warn),
        Type::Group(g) => check_type(g.elem.as_ref(), pool_type, gen_idents, warn),
        Type::Macro(m) => emit_error!(m.span(), "cannot evaluate macro type"),
        _ => emit_error!(ty.span(), "cannot evaluate")
    }
}

pub fn derive_cbindgen(input: TokenStream) -> TokenStream {
    // Parse the input tokens into a syntax tree.
    let input = parse_macro_input!(input as DeriveInput);

    let mods = crate::list(&input.attrs, "mods");
    // let generics = crate::list(&input.attrs, "generics");
    let attrs = crate::list(&input.attrs, "attrs");
    // let mut has_generics = false;
    let mut warn_no_generics = true;
    let mut warn_bare_generics = true;
    let mut is_concurrent = false;
    attrs.iter().for_each(|attr| {
        let s = attr.to_string();
        if s == "allow_no_generics" {
            warn_no_generics = false;
        } else if s == "allow_generics" {
            warn_bare_generics = false;
        } else if s == "concurrent" {
            is_concurrent = true;
        } else {
            abort!(
                attr.span(), "undefined attribute `{}`", attr.to_string();
                note = "available attributes are `concurrent`, `allow_no_generics`, and `allow_generics`"
            )
        }
    });
    let mut gen_idents = vec!();
    let mut found_pool_generic: Option<Ident> = None;
    let mut ogen = vec!();
    if !input.generics.params.is_empty() {
        for t in &input.generics.params {
            if let GenericParam::Type(t) = t {
                if t.ident == "_P" {
                    emit_error!(t.ident.span(), "`_P` is reserved");
                }
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
                gen_idents.push(t.ident.clone());
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
    // if !ogen.is_empty() {
    //     let pool = if let Some(p) = &found_pool_generic { p.to_string() } else { "".to_owned() };
    //     let mut abort = false;
    //     for t in &ogen {
    //         if *t != pool {
    //             emit_warning!(t.span(),
    //                 "FFI-incompatible generic type parameter";
    //                 help = "add {} to the generics list and remove it from here; use corundum::gen::ByteArray instead", t
    //             );
    //             abort = true;
    //         }
    //     }
    //     if abort {
    //         abort!(input.ident.span(),
    //             "struct {} should have at least one generic type parameter implementing MemPool trait", input.ident;
    //             help = "use corundum::gen::ByteArray instead of the generic types, and specify the generic types using `generics(...)` attribute (e.g., #[generics({})])",
    //             ogen.iter().map(|v| v.to_string()).collect::<Vec<String>>().join(", ")
    //         )
    //     }
    // }
    if found_pool_generic.is_none() {
        abort!(input.ident.span(),
            "struct {} should be generic with regard to the pool type", input.ident;
            help = "specify a generic type which implements MemPool trait"
        )
    }
    let pool_type = found_pool_generic.expect(&format!("{}", line!()));

    if let Data::Struct(s) = input.data {
        for f in s.fields {
            check_type(&f.ty, &pool_type, &gen_idents, warn_bare_generics);
        }
    } else if let Data::Enum(e) = input.data {
        for v in e.variants {
            for f in v.fields {
                check_type(&f.ty, &pool_type, &gen_idents, warn_bare_generics);
            }
        }
    } else {
        abort_call_site!("`Export` cannot be derived for `union`")
    }

    // let mut all_pools = unsafe { match POOLS.lock() {
    //     Ok(g) => g,
    //     Err(p) => p.into_inner()
    // } };

    // for m in &mods {
    //     let entry = all_pools.entry(m.to_string()).or_insert(Contents::default());
    //     entry.pools.push(input.ident.to_string());
    // }

    let generics: Vec<Ident> = gen_idents.iter().filter_map(|v| if *v == pool_type { None } else { Some(v.clone()) }).collect();
    let mut generics_list = quote!{ template <#(class #generics,)*;>  }.to_string().replace(", ;", "")+" ";
    let mut template = quote!{ template <#(class #generics,)* class _P> }.to_string();
    let mut generics_str = quote!{ #(#generics,)* }.to_string();

    if generics.is_empty() {
        if warn_no_generics {
            emit_warning!(input.ident.span(),
                "struct {} does not have any generic type parameter as the data type", input.ident;
                help = "use `#[attrs(allow_no_generics)]` to disable this warning"
            );
        }
        generics_str = "".to_string();
        generics_list = "".to_string();
        template = "template < class _P >".to_string();
    }

    // Used in the quasi-quotation below as `#name`.
    let name = input.ident;
    let new_name = format_ident!("__{}", name);
    let name_str = name.to_string();
    let small_name = name_str.to_lowercase();
    // let cname = format!("p{}_t", small_name);
    let cname = format!("{}", name);

    let mut expanded = vec![];
    let mut includes = "".to_owned();
    let mut all_types = unsafe { match TYPES.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner()
    } };
    let mut entry = all_types.entry(name_str.clone()).or_insert(Contents::default());
    entry.generics = generics.iter().map(|v| v.to_string()).collect();
    let new_sizes: Vec<Ident>  = generics.iter().map(|v| format_ident!("{}_size", v.to_string())).collect();

    let size_list: Vec<String> = new_sizes.iter().map(|v| v.to_string()).collect();
    let mut size_list = size_list.join(", ");
    if !new_sizes.is_empty() { size_list += ", "; }
    let size_list_arg: Vec<String> = new_sizes.iter().map(|v| format!("size_t {}", v)).collect();
    let mut size_list_arg = size_list_arg.join(", ");
    if !new_sizes.is_empty() { size_list_arg += ", "; }
    let sizeof_list: Vec<String> = generics.iter().map(|v| format!("sizeof({})", v)).collect();
    let mut sizeof_list = sizeof_list.join(", ");
    if !new_sizes.is_empty() { sizeof_list += ", "; }

    let mut conc_decl = "";
    let mut lock = "";
    let mut other_lock = "";
    let mut guard_fn = "";

    if is_concurrent {
        entry.attrs.concurrent = true;
        conc_decl = "
    carbide::recursive_mutex<_P> __mu;";
        lock = "
        carbide::mutex_locker<_P> lock(&this->__mu);";
        other_lock = "
        carbide::mutex_locker<_P> lock(&const_cast<Self&>(other).__mu);";
        guard_fn = "

    inline carbide::mutex_locker<_P> guard() const {
        return &this->__mu;
    }";
    }

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
                use corundum::*;
                use std::os::raw::c_char;
                use std::ffi::CStr;
                use std::collections::hash_map::DefaultHasher;
                use std::hash::{Hash, Hasher};

                #[allow(non_camel_case_types)]
                type #m = super::#m::Allocator;

                #[no_mangle]
                pub extern "C" fn #fn_new(#(#new_sizes: usize,)* j: *const c_void) -> *const #new_name<#m> {
                    use corundum::Pbox;
                    use corundum::MemPoolTraits;

                    assert!(!j.is_null(), "transactional operation outside a transaction");
                    unsafe {
                        let j = corundum::utils::read::<corundum::stm::Journal<#m>>(j as *mut u8);
                        #m::new(#new_name::new(#(#new_sizes,)* j), j)
                    }
                }

                #[no_mangle]
                pub extern "C" fn #fn_drop(obj: *mut #new_name<#m>) {
                    use corundum::Pbox;

                    assert!(!obj.is_null(),);
                    unsafe {
                        Pbox::<#new_name<#m>,#m>::from_raw(obj); // drops when out of scope
                    }
                }

                #[no_mangle]
                pub extern "C" fn #fn_open(p: &#__m, #(#new_sizes: usize,)* name: *const c_char) -> *const #new_name<#m> {
                    let name = unsafe { CStr::from_ptr(name).to_str().expect(&format!("{}", line!())) };
                    let mut hasher = DefaultHasher::new();
                    name.hash(&mut hasher);
                    let key = hasher.finish();

                    let mut res: *const #new_name<#m> = std::ptr::null();
                    if transaction(AssertTxInSafe(|j| {
                        let mut objs = p.objs.lock(j);
                        let obj = objs.get_or_insert(key, || {
                            #__mod::RootObject::#name(#new_name::new(#(#new_sizes,)* j))
                        }, j);
                        if let #__mod::RootObject::#name(obj) = &obj {
                            res = obj as *const #new_name<#m>;
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
    static const {name}<{pool}>* __create({size_list_arg}const journal *j) {{
        return {fn_new}({size_list}j);
    }}
    static void drop({name}<{pool}> *obj) {{
        {fn_drop}(obj);
    }}
    static const {name}<{pool}>* open(const {root_name} *p, {size_list_arg}const char *name) {{
        return {fn_open}(p, {size_list}name);
    }}
    // specialized methods
}};\n",
small_name = small_name,
name = new_name,
pool = pool,
size_list = size_list,
size_list_arg = size_list_arg,
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
#include <carbide>
#include <unordered_set>
#include <pstdlib>
#include <cstring>
{includes}

template < class _P >
struct {small_name}_traits {{
    typedef typename pool_traits<_P>::journal journal;
    static const {name}<_P>* __create({size_list_arg}const journal *j);
    static void drop({name}<_P> *obj);
    static const {name}<_P>* open(const void *p, {size_list_arg}const char *name);
    // template constructor
    // template methods
}};

{template}
class {cname} : public carbide::psafe_type_parameters {{

    typedef pool_traits<_P>  pool_traits;
    typedef typename pool_traits::handle  handle;
    typedef typename pool_traits::journal  journal;
    typedef carbide::pointer_t<{name}<_P>, _P>  pointer;
    using Self = {cname};

    pointer inner;
    typename carbide::make_persistent<std::string, _P>::type name;
    bool moved;
    bool is_root;{conc_decl}

    static std::unordered_set<std::string> objs;

private:
    inline {name}<_P>* self() const {{
        return const_cast<{name}<_P>*>(inner.operator->());
    }}

public:
    explicit {cname}(const journal *j, const std::string &name = \"(anonymous)\") {{
        inner = pointer::from({small_name}_traits<_P>::__create({sizeof_list}j));
        this->name = name.c_str();
        is_root = false;
        moved = false;
    }}

    explicit {cname}(const {cname} &other) {{{other_lock}
        if (pool_traits::valid(this)) {{
            assert(!other.moved, \"object was already moved\");
            const_cast<Self&>(other).moved = true;
        }}
        inner = other.inner;
        name = other.name;
        is_root = false;
        moved = false;
    }}

    explicit {cname}(const handle *pool, const std::string &name) noexcept(false) {{{lock}
        _P::txn([this,&pool,&name](auto j) {{
            assert(objs.find(name) == objs.end(), \"'%s' was already open\", name.c_str());
            this->name = name.c_str();
            objs.insert(name);
            pointer obj = pointer::from_unsafe({small_name}_traits<_P>::open(pool, {sizeof_list}name.c_str()));
            memcpy((void*)&inner, (void*)&obj, sizeof(pointer));
            is_root = true;
        }} );
    }}

    // other constructors

    {cname}() = delete;
    {cname} &operator= ({cname} &&) = delete;
    void *operator new (size_t) = delete;
    void *operator new[] (size_t) = delete;
    void  operator delete[] (void*) = delete;
    void  operator delete   (void*) = delete;

    ~{cname}() noexcept(false) {{{lock}
        if (!moved && pool_traits::valid(this)) {{
            if (is_root) {{
                auto n = name.c_str();
                assert(objs.find(n) != objs.end(), \"'%s' is not open\", n);
                objs.erase(n);
            }} else {{
                {small_name}_traits<_P>::drop(
                    static_cast<{name}<_P>*>(
                        static_cast<void*>(inner)
                    )
                );
            }}
        }}
    }}{guard_fn}

    // other methods
}};

{template} std::unordered_set<std::string> {cname}<{generics}_P>::objs;
",
template = template,
generics = generics_str,
includes = includes,
sizeof_list = sizeof_list,
size_list_arg = size_list_arg,
small_name = small_name,
name = new_name,
cname = cname,
conc_decl = conc_decl,
lock = lock,
guard_fn = guard_fn,
other_lock = other_lock
);
    entry.decl = format!("{template} class {name};",
            name = name,
            template = template
        );
    entry.alias = format!("{generics_list}using {name} = {name}<{generics}{{pool}}>;",
            name = name,
            generics = generics_str,
            generics_list = generics_list
        );
    
    let gen: Vec<TokenStream2> = gen_idents.iter().map(|v| if *v == pool_type { quote!(P) } else { quote!(corundum::c_void) } ).collect();
    let expanded = quote! {
        pub type #new_name<P: corundum::MemPool> = #name<#(#gen,)*>;
        #(#expanded)*
    };

    // Hand the output tokens back to the compiler.
    TokenStream::from(expanded)
}

fn refine_path(m: &TokenStream2, p: &mut Path, tmpl: &Vec<String>, ty_tmpl: &Ident, gen: &Vec<String>, check: i32, modify: bool, has_generics: &mut Option<&mut bool>, ident: &Ident) {
    for s in &mut p.segments {
        match &mut s.arguments {
            PathArguments::AngleBracketed(args) => {
                for g in &mut args.args {
                    match g {
                        GenericArgument::Type(ty) => { check_generics(m, ty, &tmpl, ty_tmpl, gen, if s.ident != "Gen" && check == 1 { 1 } else { 0 }, modify, has_generics, ident); }
                        GenericArgument::Binding(b) => { check_generics(m, &mut b.ty, &tmpl, ty_tmpl, gen, check, modify, has_generics, ident); }
                        _ => ()
                    }
                }
            }
            PathArguments::Parenthesized(args) => {
                for i in &mut args.inputs { check_generics(m, i, tmpl, ty_tmpl, gen, check, modify, has_generics, ident); }
                if let ReturnType::Type(_, ty) = &mut args.output {
                    check_generics(m, ty, tmpl, ty_tmpl, gen, check, modify, has_generics, ident);
                }
            }
            _ => ()
        }
    }
}

fn check_generics(m: &TokenStream2, ty: &mut Type, tmpl: &Vec<String>, ty_tmpl: &Ident, gen: &Vec<String>, check: i32, modify: bool, has_generics: &mut Option<&mut bool>, ident: &Ident) -> bool {
    let res = match ty {
        Type::Array(a) => check_generics(m, &mut *a.elem, tmpl, ty_tmpl, gen, check, modify, has_generics, ident),
        Type::BareFn(f) => {
            for i in &mut f.inputs {
                if check_generics(m, &mut i.ty, tmpl, ty_tmpl, gen, 2, modify, has_generics, &format_ident!("j")) {
                    abort!(
                        i.ty.span(), "no bindings found for template parameters";
                        note = "use template types in form of references or pointers"
                    );
                }
            }
            if let ReturnType::Type(_, ty) = &mut f.output {
                check_generics(m, ty, tmpl, ty_tmpl, gen, 2, modify, has_generics, ident);
            }
            false
        },
        Type::Group(g) => check_generics(m, &mut *g.elem, tmpl, ty_tmpl, gen, check, modify, has_generics, ident),
        Type::Paren(ty) => check_generics(m, &mut *ty.elem, tmpl, ty_tmpl, gen, check, modify, has_generics, ident),
        Type::Path(p) => {
            // if let Some(last) = p.path.segments.last() {
            //     if last.ident == "Box" {
            //         if let PathArguments::AngleBracketed(args) = &last.arguments {
            //             let args = &args.args;
            //             *ty = parse2(quote!(*const #args)).expect(&format!("{}", line!()));
            //             return check_generics(m, ty, tmpl, ty_tmpl, gen, check, modify, has_generics);
            //         }
            //     }
            // }
            if let Some(last) = p.path.segments.last() {
                if last.ident == "Journal" {
                    let args = &last.arguments;
                    let j = quote!(Journal#args);
                    if j.to_string() != quote!(Journal<#ty_tmpl>).to_string() {
                        abort!{
                            ty.span(), "invalid use of `Journal`";
                            help = "consider using `Journal<{}>`", ty_tmpl
                        }
                    }
                }
            }
            if p.path.segments.len() == 1 {
                if p.path.segments[0].arguments == PathArguments::None {
                    let name = p.path.segments[0].ident.to_string();
                    if tmpl.contains(&name) && *ty_tmpl != name {
                        // if !gen.contains(&name) {
                        //     abort!(
                        //         ty.span(), "template parameter `{}` is not in the generic type list", &name;
                        //         note = "consider adding `{}` to the `generics()` attribute of the implementing type, or choose from {}", &name, gen.join(", ")
                        //     );
                        // }
                        if modify {
                            *ty = parse2(quote!(corundum::c_void)).expect(&format!("{}", line!()));
                        }
                        if let Some(has_generics) = has_generics {
                            **has_generics = true;
                        }
                        true
                    } else if *ty_tmpl == name {
                        if modify {
                            *ty = parse2(quote!(#m)).expect(&format!("{}", line!()));
                        }
                        false
                    } else {
                        false
                    }
                } else {
                    refine_path(m, &mut p.path, tmpl, ty_tmpl, gen, check, modify, has_generics, ident);
                    false
                }
            } else {
                refine_path(m, &mut p.path, tmpl, ty_tmpl, gen, check, modify, has_generics, ident);
                false
            }
        }
        // tmpl.contains(&p.path.get_ident().expect(&format!("{}", line!())).ident.to_string()),
        Type::Ptr(p) => {
            if check_generics(m, &mut *p.elem, tmpl, ty_tmpl, gen, if check == 2 { 0 } else { check }, modify, has_generics, ident) {
                // update(ty);
                // *ty = parse2(quote!(corundum::gen::Gen)).expect(&format!("{}", line!()));
                // if modify {
                //     // *ty = parse2(quote!(corundum::gen::Gen)).expect(&format!("{}", line!()));
                //     *p.elem = parse2(quote!(corundum::c_void)).expect(&format!("{}", line!()));
                // }
            }
            false
        },
        Type::Reference(r) =>  {
            if check_generics(m, &mut *r.elem, tmpl, ty_tmpl, gen, if check == 2 { 0 } else { check }, modify, has_generics, ident) {
                // update(ty);
                // *ty = parse2(quote!(corundum::gen::Gen)).expect(&format!("{}", line!()));
                // if modify {
                //     // *ty = parse2(quote!(corundum::gen::Gen)).expect(&format!("{}", line!()));
                //     *r.elem = parse2(quote!(corundum::c_void)).expect(&format!("{}", line!()));
                // }
            }
            false
        },
        Type::Slice(s) => check_generics(m, &mut *s.elem, tmpl, ty_tmpl, gen, check, modify, has_generics, ident),
        Type::Tuple(t) => t.elems.iter_mut().any(|t| check_generics(m, t, tmpl, ty_tmpl, gen, check, modify, has_generics, ident)),
        Type::Verbatim(v) => {
            let name = v.to_string();
            if tmpl.contains(&name) {
                // if modify {
                //     *ty = parse2(quote!(corundum::c_void)).expect(&format!("{}", line!()));
                // }
                if let Some(has_generics) = has_generics {
                    **has_generics = true;
                }
                true
            } else if *ty_tmpl == name {
                if modify {
                    *ty = parse2(quote!(#m)).expect(&format!("{}", line!()));
                }
                false
            } else {
                false
            }
        },
        _ => false
    };
    if res && check == 1 {
        abort!(
            ty.span(), "no bindings found for template parameters";
            note = "consider using corundum::gen::Gen<{}, {}>", quote!(#ty), ty_tmpl
        );
    }
    res
}

pub fn cbindgen(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let mut expanded = vec![];
    let extern_mod;
    if let Ok(imp) = parse2::<ItemImpl>(item.clone().into()) {
        extern_mod = format_ident!("__extern_mod_{}_{}", imp.span().start().line, imp.span().start().column);
        let mut generics = vec![];
        if let Type::Path(ref tp) = *imp.self_ty {
            let mut pool_type = None;
            imp.generics.params.iter().for_each(|t|
                if let GenericParam::Type(t) = t {
                    if t.ident == "_P" {
                        emit_error!(t.ident.span(), "`_P` is reserved");
                    }
                    generics.push(t.ident.clone());
                    if t.bounds.iter().any(|b| if let TypeParamBound::Trait(t) = b {
                            if let Some(ident) = t.path.get_ident() {
                                ident == "MemPool"
                            } else {
                                false
                            }
                        } else {
                            false
                        }
                    ) {
                        pool_type = Some(t.ident.clone());
                    } else {
                        if let Some(w) = &imp.generics.where_clause {
                            for p in &w.predicates {
                                if let WherePredicate::Type(t) = p {
                                    for b in &t.bounds {
                                        if let TypeParamBound::Trait(tr) = b {
                                            if let Some(ident) = tr.path.get_ident() {
                                                if ident == "MemPool" {
                                                    if let Type::Path(p) = &t.bounded_ty {
                                                        pool_type = p.path.get_ident().map(|v| v.clone());
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
            );

            if pool_type.is_none() {
                emit_error!(imp.generics.span(), "At least one parameter is needed that implements `MemPool`");
                return item;
            }
            let pool_type = pool_type.expect(&format!("{}", line!()));

            let slf = &tp.path.segments.last().expect(&format!("{}", line!()));
            let name = &slf.ident;
            let new_name = format_ident!("__{}", name);
            let small_name = name.to_string().to_lowercase();
    
            let mut types = unsafe { match TYPES.lock() {
                Ok(g) => g,
                Err(p) => p.into_inner()
            }};
    
            let entry = types.entry(name.to_string()).or_insert(Contents::default());

            let mut ty_spec: Vec<&Ident> = vec![];
            if let PathArguments::AngleBracketed(args) = &slf.arguments {
                args.args.iter().for_each(|v| {
                    if let GenericArgument::Type(t) = v {
                        if let Type::Path(p) = t {
                            if let Some(i) = p.path.get_ident() {
                                if generics.contains(i) {
                                    ty_spec.push(i);
                                } else {
                                    emit_error!(p.span(), "partial specialization is not allowed")
                                }
                            } else {
                                emit_error!(p.span(), "partial specialization is not allowed")
                            }
                        } else {
                            emit_error!(t.span(), "partial specialization is not allowed")
                        }
                    } else {
                        emit_error!(v.span(), "only type generalization is allowed")
                    }
                });
            }
            
            for pool in &entry.pools {
                let pool = format_ident!("{}", pool);
                expanded.push(quote! {
                    #[allow(non_camel_case_types)]
                    type #pool = crate::#pool::Allocator;
                });
            }

            for fn_item in imp.items {
                if let ImplItem::Method(func) = fn_item {
                    if let Visibility::Public(_) = &func.vis {
                        let mut spc = func.clone();
                        let mut inputs = Punctuated::<_, Token![,]>::new();
                        let mut args = vec!();
                        let mut is_constructor = false;
                        if let ReturnType::Type(_, ty) = &mut spc.sig.output {
                            if let Type::Path(s) = &**ty {
                                if let Some(id) = s.path.get_ident() {
                                    if id == "Self" {
                                        is_constructor = true;
                                        **ty = parse2(quote!(corundum::c_void)).expect(&format!("{}", line!()));
                                    }
                                }
                            }
                        }
                        let mut gen: Template = spc.sig.generics.params.iter()
                            .filter_map(|i|
                                if let GenericParam::Type(t) = i {
                                    Some(t.ident.to_string())
                                } else {
                                    None
                                }
                            ).collect();
                        for i in &generics {
                            if *i != pool_type {
                                gen.push(i.to_string());
                            }
                        }
                        {
                            if is_constructor {
                                let mut i = spc.sig.inputs.iter_mut();
                                if let Some(a) = i.nth(0) {
                                    if let FnArg::Typed(PatType { pat, ty, .. }) = a {
                                        if let Pat::Ident(PatIdent { ident, .. }) = &**pat {
                                            let mut has_generics = false;
                                            check_generics(&quote!(), &mut *ty, &gen, &pool_type, &entry.generics, 1, false, &mut Some(&mut has_generics), &ident);
                                            args.push((has_generics, ident.to_string()));
                                        }
                                    }
                                    inputs.push(a.clone());
                                }
                            }
                            let mut i = spc.sig.inputs.iter_mut();
                            if i.next().is_some() {
                                while let Some(a) = i.next() {
                                    if let FnArg::Typed(PatType { pat, ty, .. }) = a {
                                        if let Pat::Ident(PatIdent { ident, .. }) = &**pat {
                                            let mut has_generics = false;
                                            check_generics(&quote!(), &mut *ty, &gen, &pool_type, &entry.generics, 1, false, &mut Some(&mut has_generics), &ident);
                                            args.push((has_generics, ident.to_string()));
                                        }
                                    }
                                    // eprintln!("fn {}: {}", spc.sig.ident, quote!(#a));
                                    inputs.push(a.clone());
                                }
                            }
                        }
                        spc.sig.generics = Generics::default();
                        let mut output_has_generics = false;
                        if let ReturnType::Type(_, ty) = &mut spc.sig.output {
                            check_generics(&quote!(), ty, &gen, &pool_type, &entry.generics, 1, false, &mut Some(&mut output_has_generics), &spc.sig.ident);
                        }
                        let mut last_arg_is_journal = false;
                        if let Some(last) = spc.sig.inputs.last() {
                            if let FnArg::Typed(ty) = last {
                                if let Type::Reference(r) = &*ty.ty {
                                    if let Type::Path(p) = &*r.elem {
                                        if let Some(last) = p.path.segments.last() {
                                            if last.ident == "Journal" {
                                                let args = &last.arguments;
                                                let j = quote!(Journal#args);
                                                last_arg_is_journal = j.to_string() == quote!(Journal<#pool_type>).to_string();
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        if is_constructor && !last_arg_is_journal {
                            emit_error! {
                                spc.sig.span(), "invalid constructor";
                                note = "the last argument of a constructor should be a `&Journal<{}>`", pool_type
                            }
                            continue;
                        }
                        let mut is_const = true;
                        if let Some(first) = spc.sig.inputs.first() {
                            if let FnArg::Receiver(rc) = first {
                                if is_constructor {
                                    emit_error! {
                                        spc.sig.span(), "ambiguous constructor";
                                        note = "a constructor cannot have a receiver (`self` argument)"
                                    }
                                    continue;
                                }
                                is_const = rc.mutability.is_none();
                                // if rc.mutability.is_some() {
                                //     emit_error!(rc.span(), "mutable receiver is not allowed");
                                //     continue;
                                // }
                            }
                        }
                        spc.sig.inputs = inputs;
                        
                        if let Ok(abi) = parse2::<Abi>(quote!(extern "C")) {
                            spc.sig.abi = Some(abi);
                        }
                        spc.block = parse2(quote!{{
                            () // no implementation
                        }}).expect(&format!("{}", line!()));

                        entry.funcs.push((
                            spc.sig.ident.to_string(),
                            args,
                            quote!(#[no_mangle] #spc).to_string(),
                            gen.clone(),
                            pool_type.to_string(),
                            spc.sig.output != ReturnType::Default,
                            output_has_generics,
                            is_constructor,
                            is_const
                        ));
    
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
                            let ident = ext.sig.ident.clone();
                            let fname = format_ident!("__{}_{}_{}", m_str, small_name, ext.sig.ident);
                            ext.sig.ident = fname.clone();
                            ext.sig.generics = Generics::default();
                            if let Ok(abi) = parse2::<Abi>(quote!(extern "C")) {
                                ext.sig.abi = Some(abi);
                            }
                            let mut has_receiver = false;
                            if let Some(first) = ext.sig.inputs.first_mut() {
                                if let FnArg::Receiver(rc) = first {
                                    has_receiver = true;
                                    let mt = rc.mutability;
                                    if let Ok(arg) = parse2::<FnArg>(quote!(__self: &#mt #new_name<#m>)) {
                                        *first = arg;
                                    }
                                }
                            }
                            if is_constructor {
                                let args = ext.sig.inputs.iter().collect::<Vec<&FnArg>>();
                                let args = args.split_last().expect(&format!("{}", line!())).1;
                                let mut failed = false;
                                let underline = format_ident!("_");
                                let vals: Vec<&Ident> = ext.sig.inputs.iter().map(|i| {
                                    if let FnArg::Typed(PatType {pat, ..}) = i {
                                        if let Pat::Ident(id) = &**pat {
                                            &id.ident
                                        } else {
                                            failed = true;
                                            emit_error!(pat.span(), "invalid input");
                                            &underline
                                        }
                                    } else {
                                        failed = true;
                                        emit_error!(i.span(), "invalid input");
                                        &underline
                                    }
                                }).collect();
                                if failed {
                                    break;
                                }
                                let vals = vals.split_last().expect(&format!("{}", line!())).1;
                                expanded.push(quote!{
                                    #[no_mangle]
                                    #[deny(improper_ctypes_definitions)]
                                    pub extern "C" fn #fname(#(#args,)* j: *const corundum::c_void) -> *mut #new_name<#m> {
                                        use corundum::Pbox;
                                        use corundum::MemPoolTraits;

                                        assert!(!j.is_null(), "transactional operation outside a transaction");
                                        unsafe {
                                            let j = corundum::utils::read::<corundum::stm::Journal<#m>>(j as *mut u8);
                                            #m::new(#new_name::#ident(#(#vals,)* j), j)
                                        }
                                    }
                                });
                            } else if has_receiver {
                                let fname = func.sig.ident.clone();
                                let mut args = vec![];
                                for arg in &mut ext.sig.inputs {
                                    if let FnArg::Typed(PatType { pat, ty, .. }) = arg {
                                        if let Pat::Ident(PatIdent { ident, .. }) = &mut **pat {
                                            if ident != "__self" {
                                                check_generics(&quote!(#m), ty, &gen, &pool_type, &entry.generics, 1, true, &mut None, ident);
                                                args.push(quote!(#ident));
                                            }
                                        }
                                    }
                                }
                                if let ReturnType::Type(_, ty) = &mut ext.sig.output {
                                    check_generics(&quote!(#m), ty, &gen, &pool_type, &entry.generics, 1, true, &mut None, &func.sig.ident);
                                }
                                ext.block = parse2(quote!{{
                                    __self.#fname(#(#args,)*)
                                }}).expect(&format!("{}", line!()));
    
                                expanded.push(quote!{
                                    #[no_mangle]
                                    #[deny(improper_ctypes_definitions)]
                                    #ext
                                });
                            } else {
                                emit_error!(func.span(), "external functions should have a receiver (i.e., `self` argument)");
                                break;
                            }
                        }
                    }
                }
            }
        }
    } else {
        abort_call_site!("`export` attribute can be used only on `impl` items");
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

    if let Err(e) = create_dir_all(&dir) {
        if warning {
            emit_warning!(span, "{}", e);
        }
    }

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
                pool.contents = pool.contents.replace("// friend classes",
                    &format!("// friend classes\n    {}\n", fwd_decl.replace(">", ">\n    friend")));
            }
        }
        let lock = if cnt.attrs.concurrent { "
        auto __guard = guard();" } else { "" };
        let mut cbindfile = "".to_owned();
        // let mut funcs = vec!();
        for (_, _, f, _, _, _, _, _, _) in &mut cnt.funcs {
            let re = Regex::new(&format!(r"\#\[no_mangle\].*")).expect(&format!("{}", line!()));
            if re.find(f).is_some() {
                let re = Regex::new(&format!(r"\bGen\b\s*<\s*(\w+)\s*>")).expect(&format!("{}", line!()));
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

            for (name, fn_args, sig, tmp, ty_pool, has_return, _, is_cons, is_const) in &mut cnt.funcs {
                let tmpl = if tmp.is_empty() { "".to_owned() } else {
                    format!("template < class {} > ", tmp.join(", class "))
                };
                let tmpl_kw = if tmp.is_empty() { "" } else { "template " }.to_owned();
                let gen = if tmp.is_empty() { "".to_owned() } else {
                    format!("<{}>", tmp.join(","))
                }.to_owned();
                let args = fn_args.iter().map(|(_, n)| n.to_owned()).collect::<Vec<String>>().join(", ");
                let re = Regex::new(&format!(r"(.+\W+{}\(.*\));", name)).expect(&format!("{}", line!()));
                let sig_re = Regex::new(&format!(r".+\W+{}(\(.*\));", name)).expect(&format!("{}", line!()));
                let re_pool = if ty_pool.is_empty() { None } else {
                    Some(Regex::new(&format!(r"\b{}\b", ty_pool)).expect(&format!("{}", line!())))
                };
                if let Some(cap) = re.captures(&s) {
                    *sig = cap.get(1).expect(&format!("{}", line!())).as_str().to_owned();
                    if let Some(re) = &re_pool {
                        *sig = re.replace_all(sig, "_P").to_string();
                    }

                    let mut append;
                    if *is_cons {
                        let name_str = ty.to_string();
                        let cname = name_str;
                        let mut sig = sig_re.captures(&s).expect(&format!("{}", line!())).get(1).expect(&format!("{}", line!())).as_str().to_owned();
                        if let Some(re) = &re_pool {
                            sig = re.replace_all(&sig, "_P").to_string();
                        }
                        append = format!("    // other constructors\n    {cname}{sig}: moved(false), is_root(false) {{{lock}
        assert(!moved, \"object was already moved\");
        {ty}_traits<_P>::{tmp}{fn}{gen}(&inner{comma}{args});
    }}",
                            sig=sig,
                            ty = ty.to_lowercase(),
                            cname=cname,
                            tmp = tmpl_kw,
                            gen = gen,
                            fn = name,
                            comma = if args.is_empty() { "" } else { ", " },
                            args = args,
                            lock = lock,
                        );
                    } else {
                        cnt.contents = cnt.contents.replace("    // template methods",
                            &format!("    // template methods\n    {}static {};",
                            tmpl,
                            sig.replacen(
                                &format!("{}(", name),
                                &format!("{name}({const}__{ty}<_P> *__self, ",
                                name = name,
                                ty = ty,
                                const = if *is_const { "const " } else { "" }), 1)
                                .replace(", )", ")")));
                        let ret_tok = if *has_return { "return " } else { "" };
                        append = format!("    // other methods\n    {sig}{const} {{{lock}
        {ret}{ty}_traits<_P>::{tmp}{fn}{gen}(self(){comma}{args});
    }}\n",
                            ret = ret_tok,
                            ty = ty.to_lowercase(),
                            sig = sig,
                            tmp = tmpl_kw,
                            gen = gen,
                            fn = name,
                            comma = if args.is_empty() { "" } else { ", " },
                            const = if *is_const { " const" } else { "" },
                            // self = if *is_const { "self()".to_owned() } else { format!("const_cast<{}<_P>*>(self())", ty) },
                            args = args,
                            lock = lock,
                        );
                    }
                    // eprintln!("type: {:?}", cnt.generics);
                    // eprintln!("func {}: {:?}", name, tmp);
                    let diff = tmp.len() - cnt.generics.len();
                    for i in diff .. tmp.len() {
                        let re = Regex::new(&format!(r"\b{}\b", tmp[i])).expect(&format!("{}", line!()));
                        append = re.replace_all(&append, &cnt.generics[i-diff]).to_string();
                    }
                    let fn_tmp = &tmp.as_slice()[0..diff];
                    if !fn_tmp.is_empty() {
                        append = append.replacen("\n",
                            &format!("\n    template<class {}>\n", fn_tmp.join(", class")), 1);
                    }
                    if *is_cons {
                        cnt.contents = cnt.contents.replace("    // other constructors", &append);
                    } else {
                        cnt.contents = cnt.contents.replace("    // other methods", &append);
                    }
                }
                else {
                    abort_call_site!(
                        "cbindgen could not find C++ bindings for `{}::{}(...)'", ty, name;
                        note = "in type `{}'", ty;
                        note = "in function definition\n{}", *sig;
                    );
                }
                // else {
                //         emit_call_site_warning!(
                //             "FFI-incompatible function signature in {}::{}", ty, name;
                //             note = "aborting export procedure";
                //         );
                // }
            }
            
        }

        for (p, contents) in &mut cnt.traits {
            for (f, args, sig, tmp, _, ret, ret_gen, is_cons, is_const) in &mut cnt.funcs {
                let (cret, cargs) = parse_c_fn(sig, f);
                let re = Regex::new(r"\bGen\b").expect(&format!("{}", line!()));
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
                    format!("template < class {} > ", tmp.join(", class "))
                };
                let args = arglist.join(", ");
                let old_sig = sig.clone();
                let re = Regex::new(r"\b_P\b").expect(&format!("{}", line!()));
                *sig = re.replace_all(sig, p as &str).to_string();
                if *is_cons {
                    *contents = contents.replace("    // specialized methods",
                        &format!("    // specialized methods\n    {}static {} {{\n        {}\n    }}",
                            tmp,
                            sig.replacen(
                                &format!("{}(", f),
                                &format!("{fn}(carbide::pointer_t<__{ty}<{pool}>, {pool}>* __self_ptr, ", fn=f, ty=ty, pool=p), 1)
                                .replace(", )", ")"),
                            &format!("*__self_ptr = carbide::pointer_t<__{ty}<{pool}>, {pool}>::from_unsafe((void*)__{pool}_{type}_{fn}({args}));",
                                pool = p,
                                ty = ty,
                                type = ty.to_lowercase(),
                                fn = f,
                                args = args)
                            ));
                } else {
                    *contents = contents.replace("    // specialized methods",
                        &format!("    // specialized methods\n    {}static {} {{\n        {}\n    }}",
                            tmp,
                            sig.replacen(
                                &format!("{}(", f),
                                &format!("{f}({const}__{ty}<{p}> *__self, ",
                                f=f,
                                ty=ty,
                                p=p,
                                const = if *is_const { "const " } else { "" }), 1)
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
                }
                *sig = old_sig;
            }
            cnt.contents += contents;
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

    pools.clear();
    types.clear();

    Ok(())
}

static mut CODE_SEGMENT_BASE_SET: bool = false;

pub fn carbide(input: TokenStream) -> TokenStream {
    let mut types = vec!();
    let mut mods = vec!();
    let mut output = "".to_owned();
    let mut overwrite = false;
    // let mut warnings = true;
    let parser = Punctuated::<Expr, Token![;]>::parse_terminated;
    if let Ok(segments) = parser.parse2(input.into()) {
        for segment in segments {
            if let Ok(Expr::Call(call)) = parse2(quote!(#segment)) {
                if let Expr::Path(func) = *call.func {
                    if let Some(op) = func.path.get_ident() {
                        match op.to_string().as_str() {
                            "mods"       => {
                                for arg in call.args {
                                    if let Expr::Call(call) = arg {
                                        let m = &*call.func;
                                        let mut flags = vec!();
                                        for flag in &call.args {
                                            flags.push(quote_spanned!(flag.span() => #flag));
                                        }
                                        mods.push((quote_spanned!(call.span() => #m), quote!(#(#flags)|*)));
                                    } else if let Expr::Path(m) = arg {
                                        mods.push((quote_spanned!(m.span() => #m), quote!( corundum::open_flags::O_CFNE )));
                                    }
                                }
                            },
                            "types"      => {
                                for ty in call.args {
                                    types.push(quote_spanned!(ty.span() => #ty));
                                }
                            },
                            "open_flags" => {
                            },
                            "allow" => {
                                for arg in call.args {
                                    if !if let Expr::Path(ref p) = arg {
                                        if let Some(op) = p.path.get_ident() {
                                            match op.to_string().as_str() {
                                                "overwrite" => { overwrite = true; true},
                                                // "warnings" => { warnings = false; true},
                                                _ => false
                                            }
                                        } else { false }
                                    } else { false } {
                                        abort!(arg.span(), "invalid argument";
                                            note = "available argument is 'overwrite'";
                                        );
                                    }
                                }
                            },
                            "output"     => {
                                if !if call.args.len() == 1 {
                                    if let Expr::Lit(lit) = &call.args[0] {
                                        if let Lit::Str(s) = &lit.lit {
                                            output = s.value();
                                            true
                                        } else { false }
                                    } else { false }
                                } else { false } {
                                    abort!(func.span(),
                                        "expected 1 string argument, found {} (probably non-string)", call.args.len()
                                    )
                                }
                            },
                            _ => {
                                abort!(op.span(), "invalid option";
                                    note = "available options are 'mods', 'types', 'output', and 'allow'"
                                )
                            }
                        }
                    } else {
                        abort!(func.span(), "invalid option";
                            note = "available options are 'mods', 'types', 'output', and 'allow'";
                        );
                    }
                } else {
                    abort!(call.span(), "invalid option";
                        note = "available options are 'mods', 'types', 'output', and 'allow'";
                    );
                }
            } else {
                abort!(segment.span(), "invalid input";
                    note = "carbide accepts multiple ';'-separated segments";
                    note = "available options are 'mods', 'types', 'output', and 'allow'";
                );
            }
        }
    } else {
        abort_call_site!("invalid input";
            note = "carbide accepts multiple ';'-separated segments";
            note = "available options are 'mods', 'types', 'output', and 'allow'";
        );
    }

    let recurse = types.iter().map(|name| {
        let name_str = name.to_string().replace(" ", "");
        let mut parts: Vec<&str> = name_str.split("::").collect();
        let ident = format_ident!("{}", parts.last().expect(&format!("{}", line!())));
        let new_name = format!("__{}", ident);
        parts.pop();
        parts.push(&new_name);
        let parts = parts.join("::");
        let parts: TokenStream2 = parse_str(&parts).expect(&format!("{}", line!()));
        quote_spanned!(name.span() => #ident(super::#parts<Allocator>))
    });
    let types = quote! {
        #(#recurse,)*
    };

    let mut all_pools = unsafe { match POOLS.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner()
    } };
    let mut expanded = vec![];
    for m in &mods {
        let flags = &m.1;
        let m = &m.0;
        let name_str: String = m.to_string();
        let fn_open = format_ident!("{}_open", name_str);
        let fn_close = format_ident!("{}_close", name_str);
        let fn_base = format_ident!("{}_base", name_str);
        let fn_alloc = format_ident!("{}_alloc", name_str);
        let fn_dealloc = format_ident!("{}_dealloc", name_str);
        let fn_allocated = format_ident!("{}_allocated", name_str);
        let fn_valid = format_ident!("{}_valid", name_str);
        let fn_txn = format_ident!("{}_txn", name_str);
        let fn_txn_begin = format_ident!("{}_txn_begin", name_str);
        let fn_txn_commit = format_ident!("{}_txn_commit", name_str);
        let fn_txn_rollback = format_ident!("{}_txn_rollback", name_str);
        let fn_journal = format_ident!("{}_journal", name_str);
        let fn_txn_running = format_ident!("{}_txn_running", name_str);
        let fn_log = format_ident!("{}_log", name_str);
        let fn_gen = format_ident!("{}_gen", name_str);
        let fn_print_info = format_ident!("{}_print_info", name_str);
        let fn_used = format_ident!("{}_used", name_str);
        let fn_read64 = format_ident!("{}_read64", name_str);
        let named_open = format_ident!("{}_named_open", name_str);
        let named_data_pointer = format_ident!("{}_named_data_pointer", name_str);
        let named_logged_pointer = format_ident!("{}_named_logged_pointer", name_str);
        let mod_name = format_ident!("__{}", name_str);
        let root_name = format_ident!("__{}_root_t", name_str);
        
        let entry = all_pools.entry(name_str.clone()).or_insert(Contents::default());

        expanded.push(quote! {
            corundum::pool!(#m);

            pub mod #mod_name {
                use super::*;
                use corundum::c_void;
                use corundum::ptr::Ptr;
                use corundum::stm::{Logger, Notifier};
                use corundum::stl::HashMap as PHashMap;
                use corundum::gen::ByteArray;
                use std::collections::hash_map::DefaultHasher;
                use std::hash::{Hash, Hasher};
                use std::os::raw::c_char;
                use std::ffi::CStr;
                use super::#m::*;

                #[allow(non_camel_case_types)]
                type #m = super::#m::Allocator;

                pub enum RootObject {
                    #types
                    Custom(Named),
                    None
                }

                pub struct #root_name {
                    pub(crate) objs: PMutex<PHashMap<u64, RootObject, Allocator>>
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
                    let path = unsafe { CStr::from_ptr(path).to_str().expect(&format!("{}", line!())) }.clone();
                    if flags == 0 { flags = #flags; }
                    let res = Allocator::open::<#root_name>(path, flags).expect(&format!("{}", line!()));
                    let p = &*res as *const #root_name;
                    std::mem::forget(res); // Keep the pool open
                    p
                }

                #[no_mangle]
                pub extern "C" fn #fn_close() -> bool {
                    if Allocator::is_open() {
                        unsafe { Allocator::close().expect(&format!("{}", line!())); }
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
                    unsafe {
                        let j = Journal::current(false)
                            .expect(&format!("{} cannot be used outside a transaction", stringify!(#fn_alloc)));
                        Allocator::new_uninit_for_layout(size, &*j.0) as *mut c_void
                    }
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
                        let j = Journal::current(true).expect(&format!("{}", line!()));
                        *j.1 += 1;
                        let journal = corundum::utils::as_mut(j.0);
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
                pub extern "C" fn #fn_txn(f: extern fn(*const corundum::c_void)->corundum::c_void) {
                    Allocator::transaction(|j| {
                        f(j as *const _ as *const corundum::c_void);
                    }).expect(&format!("{}", line!()));
                }

                #[no_mangle]
                pub extern "C" fn #fn_journal(create: bool) -> *const c_void {
                    unsafe {
                        if let Some(j) = Journal::current(create) {
                            let journal = corundum::utils::as_mut(j.0);
                            journal as *const _ as *const u8 as *const c_void
                        } else {
                            std::ptr::null()
                        }
                    }
                }

                #[no_mangle]
                pub extern "C" fn #fn_txn_running() -> bool {
                    Journal::is_running()
                }

                #[no_mangle]
                pub extern "C" fn #fn_log(obj: *const c_void, logged: *const u8, size: usize, j: *const c_void) {
                    assert!(!obj.is_null() && !j.is_null(), "unable to log due to null pointers");
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
                pub extern "C" fn #fn_gen() -> u32 {
                    Allocator::gen()
                }

                #[no_mangle]
                pub extern "C" fn #fn_print_info() {
                    Allocator::print_info()
                }

                #[no_mangle]
                pub extern "C" fn #fn_used() -> usize {
                    Allocator::used()
                }

                #[no_mangle]
                pub extern "C" fn #fn_read64(addr: u64) -> u64 {
                    unsafe { *Allocator::get_unchecked(addr) }
                }

                pub struct Named(u8, ByteArray<corundum::c_void, Allocator>);

                #[no_mangle]
                pub extern "C" fn #named_open(p: &#root_name, name: *const c_char, size: usize, init: extern fn(*mut c_void)->()) -> *const c_void /* Named */ {
                    use corundum::gen::Allocatable;
                    let name = unsafe { CStr::from_ptr(name).to_str().expect(&format!("{}", line!())) };
                    let mut hasher = DefaultHasher::new();
                    name.hash(&mut hasher);
                    let key = hasher.finish();

                    let mut res: *const Named = std::ptr::null();
                    if transaction(AssertTxInSafe(|j| {
                        let mut objs = p.objs.lock(j);
                        if let RootObject::Custom(named) = objs.get_or_insert(key, || unsafe {
                            let mut obj = ByteArray::<corundum::c_void, Allocator>::alloc(size, j);
                            init(obj.get_ptr_mut());
                            RootObject::Custom(Named(0, obj))
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
                    obj.1.get_ptr()
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

#include <functional>
#include <stdio.h>
#include <execinfo.h>
#include <signal.h>
#include <stdlib.h>
#include <unistd.h>
#include <proot.h>
#include <gen.h>
#include <pstdlib>
#include <carbide>
#include <unordered_set>

// forward declarations
template < class P > class Journal;

template<>
struct pool_traits<{pool}> {{
    using journal = Journal<{pool}>;
    using handle = {root_name};
    using void_pointer = carbide::pointer_t<void, {pool}>;

    static const journal* journal_handle() {{
        return (const journal*) {pool_journal}(false);
    }}
private:
    static size_t base;
    static u_int32_t gen;
    static std::unordered_set<std::string> objs;

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
    static bool txn_running() {{
        return {pool_txn_running}();
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
    static void early_start_transaction() {{
        {pool_journal}(true);
    }}
    static size_t used() {{
        return {pool_used}();
    }}

    // friend classes
    template < class T, class _P >
    friend class carbide::pointer_t;

    template < class T, class _P >
    friend class carbide::reference;

    template < class T, class _P >
    friend class carbide::cell;
    
    template < class T, class _P >
    friend class carbide::allocator;

    template < class T, class _P >
    friend class carbide::char_traits;

    template < class T, class _P >
    friend class carbide::log_traits;

    template < class _P >
    friend class carbide::recursive_mutex;

    template < class T, class _P >
    friend class proot_t;

    template < class T, class _P >
    friend class Gen;

    friend class {pool};
}};

std::unordered_set<std::string> pool_traits<{pool}>::objs;
size_t pool_traits<{pool}>::base = 0;
u_int32_t pool_traits<{pool}>::gen = 0;

class {pool}: public carbide::pool_type {{
    const {root_name} *inner;
public:
    typedef pool_traits<{pool}>::journal journal;
    // type aliases
    template < class T > using root = proot_t<T, {pool}>;
    template < class T > using make_persistent = carbide::make_persistent<T, {pool}>;
    template < class T > using cell = carbide::cell<T, {pool}>;
    
    {pool}(const char* path, u_int32_t flags, bool check_open = true) {{
        if (check_open) assert(pool_traits<{pool}>::base==0, \"{pool} was already open\");
        inner = {pool_open}(path, flags);
        pool_traits<{pool}>::base = {pool_base}();
        pool_traits<{pool}>::gen = {pool_gen}();
        __setup_codesegment_base(__get_codesegment_base());
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
pool_used = fn_used.to_string(),
pool_journal = fn_journal.to_string(),
pool_txn_running = fn_txn_running.to_string(),
pool_open = fn_open.to_string(),
pool_close = fn_close.to_string(),
pool_base = fn_base.to_string(),
pool_gen = fn_gen.to_string(),
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

    unsafe {
        if ! CODE_SEGMENT_BASE_SET {
            expanded.push(quote!(
                #[no_mangle]
                pub unsafe extern "C" fn __setup_codesegment_base(offset: i64) {
                    corundum::gen::CODE_SEGMENT_BASE = offset;
                }
            ));
            CODE_SEGMENT_BASE_SET = true;
        }
    }

    let expanded = quote! {
        #(#expanded)*

        generate!(path=#output, overwrite=#overwrite, warning=false);
    };

    // Hand the output tokens back to the compiler.
    TokenStream::from(expanded)
}