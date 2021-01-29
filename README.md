[![DOI](https://zenodo.org/badge/DOI/10.5281/zenodo.4329841.svg)](https://doi.org/10.5281/zenodo.4329841)
[![crates.io](https://img.shields.io/crates/v/corundum.svg)](https://crates.io/crates/corundum)
[![GHA build status](https://github.com/NVSL/Corundum/workflows/CI/badge.svg)](https://github.com/NVSL/Corundum/actions)
[![Build Status](https://travis-ci.org/NVSL/Corundum.svg?branch=main)](https://travis-ci.org/NVSL/Corundum)

# Corundum: A Persistent Memory Programming Library in Rust

Corundum provides persistent memory support for Rust applications. This
is useful for developing safe persistent memory applications without concerning
much about crash consistency and data loss. More details of its design and implementation
will be available in our ASPLOS 2020 paper (Please read the
[extended abstract](https://asplos-conference.org/abstracts/asplos21-paper171-extended_abstract.pdf)
for a brief overview on Corundum).

Carefully using Rust's strict type checking rules and borrowing mechanism,
Corundum guarantees that the implementation is free of common persistent memory
related bugs. Corundum leaves the software implementation with zero persistent
memory related problems of the following types:

* A persistent pointer pointing to the demote heap,
* Cross pool pointers,
* Unrecoverable modification to data,
* Data inconsistency due to power-failure,
* plus All memory-related issues that Rust handles.

Developers will see these issues during the design time. Therefore, it lowers
the risk of making mistakes. Corundum's programming model consists of using safe
persistent pointers and software transactional memory.

Three pointer-wrappers lie at the heart of Corundum interface. Developers may use
them to allocate persistent memory safely.

* [`Pbox<T>`](src/boxed.rs#L108): the simplest form of dynamic allocation,
* [`Prc<T>`](src/prc.rs#L115): a single-thread reference counted pointer for shared
    persistent objects,
* [`Parc<T>`](src/sync/parc.rs#L161): a thread-safe reference-counted pointer for
    shared persistent objects.

## Dependencies

Corundum depends on some unstable features of Rust. Therefore, it requires
nightly Rust compiler [1.50.0-nightly](https://github.com/rust-lang/rust).
Please run the following commands to download the latest version of Rust (See
[https://www.rust-lang.org/tools/install] for more details).

```shell
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup default nightly
```

Corundum is also partially dependent on a few 3rd party crates which are listed
in [`Cargo.toml`](Cargo.toml#L35-L46).

## Usage

Use either of the following instructions to add Corundum in your `Cargo.toml`
dependencies section:

```toml
[dependencies]
corundum = "0.2.2"
```

Or

```toml
[dependencies]
corundum = { git = "https://github.com/NVSL/Corundum.git" }
```

If you wish to enable a feature like `pin_journals`, please add it to the
`features` attribute. For example:

```toml
[dependencies]
corundum = { version="0.2.2", features=["pin_journals", "no_pthread"] }
```

### Memory Pools

A memory pool is a type that implements all necessary interfaces for working
with persistent memory. You can use the default memory pool, or define a new
memory pool type. The latter requires your type implementing
[`MemPool`](src/alloc/pool.rs#L181) trait. Please see the
[pass-through](src/alloc/heap.rs#L19) allocator as an example. To automatically
implement a new pool type, `pool!()` macro is provided which creates a new module
with a `BuddyAlloc` type.

```rust
corundum::pool!(my_pool);
```

### Opening a memory pool file

The first thing to do is to open the memory pool file(s) before using it. You
can do this by using either `open()` or `open_no_root()` methods. The first one
returns a the `root` object given a root object type. The second one returns a
`guard` object; the pool remains open as long as the `root`/`guard` object is in
the scope. The open functions take a pool file path and a flag set to create
the pool file.

```rust
if let Ok(_) = my_pool::BuddyAlloc::open_no_root("image", O_F) {
    println!("Image file is formatted and ready to use");
} else {
    println!("No image file found");
}
```

```rust
if let Ok(root) = my_pool::BuddyAlloc::open::<Root>("image", O_F) {
    println!("Image file is formatted and the root object is created ({:?})", root);
} else {
    println!("No image file");
}
```

### PM Safe Data Structures

You may define any data structure with the given pointers, and without any raw
pointers or references. Corundum helps you to write the right code down the road.

```rust
use corundum::rc::Prc;
use corundum::cell::LogCell;

type A = BuddyAlloc;

struct MyData {
    id: i32,
    link: Option<Prc<LogRefCell<MyData, A>, A>>
}
```

You may find it disturbing to specify the pool in every type. Corundum uses type
aliasing and procedural macros to provide an easier way for defining new data
structures. The `pool!()` macro aliases all persistent types associated with the
internal pool type. For example

```rust
pool!(my_pool);
use my_pool::*;

struct MyData {
    id: i32,
    link: Option<Prc<PRefCell<MyData>>>
}
```

`PClone` and `Root` procedural macros can also be used to automatically derive
the implementation of the corresponding traits for the type.

```rust
use corundum::default::*;

#[derive(PClone, Root)]
struct MyData {
    id: i32,
    link: Option<Prc<PRefCell<MyData>>>
}
```

### Transactional Memory

Corundum does not allow any modification to the protected data outside a
transaction. To let mutably borrowing the protected data, you may wrap it in
[`LogCell`](src/stm/cell.rs#34), [`Mutex`](src/sync/mutex.rs#88), etc.,
and use their corresponding interface for interior mutability which requires a
reference to a journal object. To obtain a journal, you may use `transaction`.

```rust
transaction(|j| {
    let my_data = Prc::new(LogRefCell::new(
        MyData {
            id: 1,
            link: None
        }), j);
    let mut my_data = my_data.borrow_mut(j);
    my_data.id = 2;
})
```

## Running Experiments on Docker

We provide a [docker image](https://hub.docker.com/r/mhz88/corundum) for running
performance tests and compare Corundum with a bunch of other persistent memory
libraries. The following commands pulls the docker image and runs a container with
all dependencies and opponent libraries pre-installed.

```sh
$ sudo sysctl -w kernel.perf_event_paranoid=-1
$ wget https://raw.githubusercontent.com/NVSL/Corundum/main/eval/docker-default.json
$ docker run --security-opt \
    seccomp=./docker-default.json \
    --mount type=tmpfs,destination=/mnt/pmem0 \
    -it mhz88/corundum:latest bin/bash
```

## Documentation

Please visit the [`Documentation`](https://nvsl.github.io/Corundum/) page for
more information.

## Issues and Contribution

Please feel free to report any bug using GitHub issues.

If you have other questions or suggestions, you can contact us
at cse-nvsl-discuss@eng.ucsd.edu.

### License

Corundum crate is licensed under Apache License, Version 2.0,
(<http://www.apache.org/licenses/LICENSE-2.0>).

Unless You explicitly state otherwise, any Contribution intentionally submitted
for inclusion in the Work by You to the Licensor shall be under the terms and
conditions of this License, without any additional terms or conditions.
