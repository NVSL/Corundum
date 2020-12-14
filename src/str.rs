//! Persistent unicode string slices

use crate::convert::PFrom;
use std::string::FromUtf8Error;
use crate::alloc::MemPool;
use crate::cell::RootObj;
use crate::clone::PClone;
use crate::stm::Journal;
use crate::vec::Vec;
use std::borrow::{Cow, ToOwned};
use std::char::decode_utf16;
use std::ops::{self, Index, IndexMut, RangeBounds};
use std::str::pattern::Pattern;
use std::string::String as StdString;
use std::string::ToString as StdToString;
use std::vec::Vec as StdVec;
use std::{fmt, hash, ptr, str};

/// A UTF-8 encoded, growable string.
///
/// The `String` type is persistent string type that has ownership over the
/// contents of the string. It has a close relationship with its borrowed
/// counterpart, the primitive [`str`].
/// 
/// [`PString`] is an alias name in the pool module for `String`.
///
/// [`PString`]: ../alloc/default/type.PString.html
///
/// # Examples
///
/// You can create a `String` from a literal string with [`String::from`]:
///
/// ```
/// # use crndm::alloc::*;
/// # use crndm::str::String;
/// # use crndm::convert::PFrom;
/// Heap::transaction(|j| {
///     let hello = String::<Heap>::pfrom("Hello, world!", j);
/// }).unwrap();
/// ```
///
/// You can append a [`char`] to a `String` with the [`push`] method, and
/// append a [`&str`] with the [`push_str`] method:
///
/// ```
/// # use crndm::alloc::*;
/// # use crndm::str::String;
/// # use crndm::convert::PFrom;
/// Heap::transaction(|j| {
///     let mut hello = String::<Heap>::pfrom("Hello, ", j);
///
///     hello.push('w', j);
///     hello.push_str("orld!", j);
/// }).unwrap();
/// ```
///
/// [`String::from`]: #method.from
/// [`char`]: std::char
/// [`push`]: #method.push
/// [`push_str`]: #method.push_str
///
/// If you have a vector of UTF-8 bytes, you can create a `String` from it with
/// the [`from_utf8`] method:
///
/// ```
/// # use crndm::alloc::*;
/// # use crndm::str::String;
/// Heap::transaction(|j| {
///     // some bytes, in a vector
///     let sparkle_heart = vec![240, 159, 146, 150];
///
///     // We know these bytes are valid, so we'll use `unwrap()`.
///     let sparkle_heart = String::from_utf8(sparkle_heart, j).unwrap();
///
///     assert_eq!("💖", sparkle_heart);
/// }).unwrap();
/// ```
///
/// [`from_utf8`]: #method.from_utf8
///
/// # UTF-8
///
/// `String`s are always valid UTF-8. This has a few implications, the first of
/// which is that if you need a non-UTF-8 string, consider [`OsString`]. It is
/// similar, but without the UTF-8 constraint. The second implication is that
/// you cannot index into a `String`:
///
/// ```compile_fail,E0277
/// let s = "hello";
///
/// println!("The first letter of s is {}", s[0]); // ERROR!!!
/// ```
///
/// [`OsString`]: std::ffi::OsString
///
/// Indexing is intended to be a constant-time operation, but UTF-8 encoding
/// does not allow us to do this. Furthermore, it's not clear what sort of
/// thing the index should return: a byte, a codepoint, or a grapheme cluster.
/// The [`bytes`] and [`chars`] methods return iterators over the first
/// two, respectively.
///
/// [`bytes`]: #method.bytes
/// [`chars`]: #method.chars
///
/// # Deref
///
/// `String`s implement [`Deref`]`<Target=str>`, and so inherit all of [`str`]'s
/// methods. In addition, this means that you can pass a `String` to a
/// function which takes a [`&str`] by using an ampersand (`&`):
///
/// ```
/// # use crndm::alloc::*;
/// # use crndm::str::String;
/// # use crndm::convert::PFrom;
/// fn takes_str(s: &str) { }
///
/// Heap::transaction(|j| {
///     let s = String::<Heap>::pfrom("Hello", j);
///     takes_str(&s);
/// }).unwrap();
/// ```
///
/// [`str`]: std::string::String
/// [`&str`]: std::string::String
/// [`Deref`]: std::ops::Deref
/// [`as_str()`]: #method.as_str
#[derive(PartialOrd, Eq, Ord)]
pub struct String<A: MemPool> {
    vec: Vec<u8, A>,
}

impl<A: MemPool> String<A> {
    /// Creates a new empty `String`.
    ///
    /// Given that the `String` is empty, this will not allocate any initial
    /// buffer. While that means that this initial operation is very
    /// inexpensive, it may cause excessive allocation later when you add
    /// data. If you have an idea of how much data the `String` will hold,
    /// consider the [`with_capacity`] method to prevent excessive
    /// re-allocation.
    ///
    /// [`with_capacity`]: #method.with_capacity
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```
    /// # use crndm::alloc::*;
    /// # use crndm::str::String;
    /// Heap::transaction(|j| {
    ///     let s = String::<Heap>::new(j);
    /// }).unwrap();
    /// ```
    #[inline]
    pub const fn new(j: &Journal<A>) -> String<A> {
        String { vec: Vec::new(j) }
    }

    /// Creates a new empty `String` with a particular capacity.
    ///
    /// `String`s have an internal buffer to hold their data. The capacity is
    /// the length of that buffer, and can be queried with the [`capacity`]
    /// method. This method creates an empty `String`, but one with an initial
    /// buffer that can hold `capacity` bytes. This is useful when you may be
    /// appending a bunch of data to the `String`, reducing the number of
    /// reallocations it needs to do.
    ///
    /// [`capacity`]: #method.capacity
    ///
    /// If the given capacity is `0`, no allocation will occur, and this method
    /// is identical to the [`new`] method.
    ///
    /// [`new`]: #method.new
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```
    /// # use crndm::alloc::*;
    /// # use crndm::str::String;
    /// Heap::transaction(|j| {
    ///     let mut s = String::with_capacity(10, j);
    ///
    ///     // The String<A> contains no chars, even though it has capacity for more
    ///     assert_eq!(s.len(), 0);
    ///
    ///     // These are all done without reallocating...
    ///     let cap = s.capacity();
    ///     for _ in 0..10 {
    ///         s.push('a', j);
    ///     }
    ///
    ///     assert_eq!(s.capacity(), cap);
    ///
    ///     // ...but this may make the string reallocate
    ///     s.push('a', j);
    /// }).unwrap();
    /// ```
    #[inline]
    pub fn with_capacity(capacity: usize, j: &Journal<A>) -> String<A> {
        String {
            vec: Vec::with_capacity(capacity, j),
        }
    }

    /// Creates a `String` from `&str`
    ///
    /// `s` may be in the volatile heap. `PStrong::from_str` will allocate enough
    /// space in pool `A` and places `s` into it an make a `String` out of it.
    ///
    /// # Example
    ///
    /// ```
    /// # use crndm::alloc::*;
    /// # use crndm::str::String;
    /// let hello = "Hello World!!!";
    ///
    /// Heap::transaction(|j| {
    ///     let phello = String::from_str(hello, j);
    ///     assert_eq!(hello, phello);
    /// }).unwrap();
    /// ```
    pub fn from_str(s: &str, j: &Journal<A>) -> String<A> {
        Self {
            vec: Vec::from_slice(s.as_bytes(), j),
        }
    }

    pub(crate) unsafe fn from_str_nolog(s: &str) -> String<A> {
        Self {
            vec: Vec::from_slice_nolog(s.as_bytes()),
        }
    }

    /// Converts a vector of bytes to a `String`.
    ///
    /// A string ([`String`]) is made of bytes ([`u8`]), and a vector of bytes
    /// ([`Vec<u8>`]) is made of bytes, so this function converts between the
    /// two. Not all byte slices are valid `String`s, however: `String`
    /// requires that it is valid UTF-8. `from_utf8()` checks to ensure that
    /// the bytes are valid UTF-8, and then does the conversion.
    ///
    /// If you are sure that the byte slice is valid UTF-8, and you don't want
    /// to incur the overhead of the validity check, there is an unsafe version
    /// of this function, [`from_utf8_unchecked`], which has the same behavior
    /// but skips the check.
    ///
    /// This method will take care to not copy the vector, for efficiency's
    /// sake.
    ///
    /// If you need a [`&str`] instead of a `String`, consider
    /// [`str::from_utf8`].
    ///
    /// The inverse of this method is [`into_bytes`].
    ///
    /// # Errors
    ///
    /// Returns [`Err`] if the slice is not UTF-8 with a description as to why the
    /// provided bytes are not UTF-8. The vector you moved in is also included.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```
    /// # use crndm::alloc::*;
    /// # use crndm::str::String;
    /// // some bytes, in a vector
    /// let sparkle_heart = vec![240, 159, 146, 150];
    ///
    /// Heap::transaction(|j| {
    ///     // We know these bytes are valid, so we'll use `unwrap()`.
    ///     let sparkle_heart = String::from_utf8(sparkle_heart, j).unwrap();
    ///
    ///     assert_eq!("💖", sparkle_heart);
    /// }).unwrap();
    /// ```
    ///
    /// Incorrect bytes:
    ///
    /// ```
    /// # use crndm::alloc::*;
    /// # use crndm::str::String;
    /// Heap::transaction(|j| {
    ///     // some invalid bytes, in a vector
    ///     let sparkle_heart = vec![0, 159, 146, 150];
    ///
    ///     assert!(String::from_utf8(sparkle_heart, j).is_err());
    /// }).unwrap();
    /// ```
    ///
    /// See the docs for [`FromUtf8Error`] for more details on what you can do
    /// with this error.
    ///
    /// [`from_utf8_unchecked`]: #method.from_utf8_unchecked
    /// [`String`]: struct.String.html
    /// [`u8`]: std::u8
    /// [`Vec<u8>`]: ../vec/struct.Vec.html
    /// [`str::from_utf8`]: std::str::from_utf8
    /// [`into_bytes`]: #method.into_bytes
    /// [`FromUtf8Error`]: std::string::FromUtf8Error
    /// [`Err`]: std::result::Result::Err
    #[inline]
    pub fn from_utf8(vec: StdVec<u8>, j: &Journal<A>) -> Result<String<A>, FromUtf8Error> {
        let s = StdString::from_utf8(vec)?;
        Ok(Self::from_str(&s, j))
    }

    /// Converts a slice of bytes to a persistent string, including invalid characters.
    ///
    /// Strings are made of bytes ([`u8`]), and a slice of bytes
    /// ([`&[u8]`][byteslice]) is made of bytes, so this function converts
    /// between the two. Not all byte slices are valid strings, however: strings
    /// are required to be valid UTF-8. During this conversion,
    /// `from_utf8_lossy()` will replace any invalid UTF-8 sequences with
    /// `U+FFFD REPLACEMENT CHARACTER`, which looks like this: �
    ///
    /// [`u8`]: std::u8
    /// [byteslice]: std::slice
    ///
    /// If you are sure that the byte slice is valid UTF-8, and you don't want
    /// to incur the overhead of the conversion, there is an unsafe version
    /// of this function, [`from_utf8_unchecked`], which has the same behavior
    /// but skips the checks.
    ///
    /// [`from_utf8_unchecked`]: #method.from_utf8_unchecked
    ///
    /// This function returns a [`Cow<'a, str>`]. If our byte slice is invalid
    /// UTF-8, then we need to insert the replacement characters, which will
    /// change the size of the string, and hence, require a `String`. But if
    /// it's already valid UTF-8, we don't need a new allocation. This return
    /// type allows us to handle both cases.
    ///
    /// [`Cow<'a, str>`]: ../../std/borrow/enum.Cow.html
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```
    /// # use crndm::alloc::*;
    /// # use crndm::str::String;
    /// // some bytes, in a vector
    /// let sparkle_heart = vec![240, 159, 146, 150];
    ///
    /// Heap::transaction(|j| {
    ///     let sparkle_heart = String::from_utf8_lossy(&sparkle_heart, j);
    ///
    ///     assert_eq!("💖", sparkle_heart);
    /// }).unwrap();
    /// ```
    ///
    /// Incorrect bytes:
    ///
    /// ```
    /// # use crndm::alloc::*;
    /// # use crndm::str::String;
    /// Heap::transaction(|j| {
    ///     // some invalid bytes
    ///     let input = b"Hello \xF0\x90\x80World";
    ///     let output = String::from_utf8_lossy(input, j);
    ///
    ///     assert_eq!("Hello �World", output);
    /// }).unwrap();
    /// ```
    pub fn from_utf8_lossy<'a>(v: &'a [u8], j: &Journal<A>) -> String<A> {
        Self::from_str(&StdString::from_utf8_lossy(v), j)
    }

    /// Decode a UTF-16 encoded vector `v` into a `String`, returning [`Err`]
    /// if `v` contains any invalid data.
    ///
    /// [`Err`]: ../../std/result/enum.Result.html#variant.Err
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```
    /// # use crndm::alloc::*;
    /// # use crndm::str::String;
    /// # use crndm::convert::PFrom;
    /// Heap::transaction(|j| {
    ///     // 𝄞music
    ///     let v = &[0xD834, 0xDD1E, 0x006d, 0x0075,
    ///               0x0073, 0x0069, 0x0063];
    ///     assert_eq!(String::pfrom("𝄞music", j),
    ///                String::from_utf16(v, j).unwrap());
    ///
    ///     // 𝄞mu<invalid>ic
    ///     let v = &[0xD834, 0xDD1E, 0x006d, 0x0075,
    ///               0xD800, 0x0069, 0x0063];
    ///     assert!(String::from_utf16(v, j).is_err());
    /// }).unwrap();
    /// ```
    pub fn from_utf16(v: &[u16], j: &Journal<A>) -> Result<String<A>, &'static str> {
        // This isn't done via collect::<Result<_, _>>() for performance reasons.
        // FIXME: the function can be simplified again when #48994 is closed.
        let mut ret = String::with_capacity(v.len(), j);
        for c in decode_utf16(v.iter().cloned()) {
            if let Ok(c) = c {
                ret.push(c, j);
            } else {
                return Err("FromUtf16Error");
            }
        }
        Ok(ret)
    }

    /// Decode a UTF-16 encoded slice `v` into a `String`, replacing
    /// invalid data with [the replacement character (`U+FFFD`)][U+FFFD].
    ///
    /// Unlike [`from_utf8_lossy`] which returns a [`Cow<'a, str>`],
    /// `from_utf16_lossy` returns a `String` since the UTF-16 to UTF-8
    /// conversion requires a memory allocation.
    ///
    /// [`from_utf8_lossy`]: #method.from_utf8_lossy
    /// [`Cow<'a, str>`]: ../borrow/enum.Cow.html
    /// [U+FFFD]: ../char/constant.REPLACEMENT_CHARACTER.html
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```
    /// # use crndm::alloc::*;
    /// # use crndm::str::String;
    /// # use crndm::convert::PFrom;
    /// Heap::transaction(|j| {
    ///     // 𝄞mus<invalid>ic<invalid>
    ///     let v = &[0xD834, 0xDD1E, 0x006d, 0x0075,
    ///               0x0073, 0xDD1E, 0x0069, 0x0063,
    ///               0xD834];
    ///
    ///     assert_eq!(String::pfrom("𝄞mus\u{FFFD}ic\u{FFFD}", j),
    ///                String::from_utf16_lossy(v, j));
    /// }).unwrap();
    /// ```
    #[inline]
    pub fn from_utf16_lossy(v: &[u16], j: &Journal<A>) -> String<A> {
        let s = StdString::from_utf16_lossy(v);
        String::from_str(&s, j)
    }

    /// Converts a vector of bytes to a `String` without checking that the
    /// string contains valid UTF-8.
    ///
    /// See the safe version, [`from_utf8`], for more details.
    ///
    /// [`from_utf8`]: struct.String.html#method.from_utf8
    ///
    /// # Safety
    ///
    /// This function is unsafe because it does not check that the bytes passed
    /// to it are valid UTF-8. If this constraint is violated, it may cause
    /// memory unsafety issues with future users of the `String`, as the rest of
    /// the standard library assumes that `String`s are valid UTF-8.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```
    /// # use crndm::alloc::*;
    /// # use crndm::str::String;
    /// Heap::transaction(|j| {
    ///     // some bytes, in a vector
    ///     let sparkle_heart = vec![240, 159, 146, 150];
    ///
    ///     let sparkle_heart = unsafe {
    ///         String::from_utf8_unchecked(sparkle_heart, j)
    ///     };
    ///
    ///     assert_eq!("💖", sparkle_heart);
    /// }).unwrap();
    /// ```
    #[inline]
    pub unsafe fn from_utf8_unchecked(bytes: StdVec<u8>, journal: &Journal<A>) -> String<A> {
        Self {
            vec: Vec::from_slice(bytes.as_slice(), journal),
        }
    }

    /// Converts a `String` into a byte vector.
    ///
    /// This consumes the `String`, so we do not need to copy its contents.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```
    /// # use crndm::alloc::*;
    /// # use crndm::str::String;
    /// # use crndm::convert::PFrom;
    /// Heap::transaction(|j| {
    ///     let s = String::<Heap>::pfrom("hello", j);
    ///     let bytes = s.into_bytes();
    ///
    ///     assert_eq!(&[104, 101, 108, 108, 111][..], &bytes[..]);
    /// }).unwrap();
    /// ```
    #[inline]
    pub fn into_bytes(self) -> Vec<u8, A> {
        self.vec
    }

    /// Extracts a string slice containing the entire `String`.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```
    /// # use crndm::alloc::*;
    /// # use crndm::str::String;
    /// # use crndm::convert::PFrom;
    ///
    /// Heap::transaction(|j| {
    ///     let s = String::<Heap>::pfrom("foo", j);
    ///
    ///     assert_eq!("foo", s.as_str());
    /// }).unwrap();
    /// ```
    #[inline]
    pub fn as_str(&self) -> &str {
        self.vec.to_str()
    }

    /// Appends a given string slice onto the end of this `String`.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```
    /// # use crndm::alloc::*;
    /// # use crndm::str::String;
    /// # use crndm::convert::PFrom;
    /// crndm::transaction(|j| {
    ///     let mut s = String::<Heap>::pfrom("foo", j);
    ///
    ///     s.push_str("bar", j);
    ///
    ///     assert_eq!("foobar", s);
    /// }).unwrap();
    /// ```
    #[inline]
    pub fn push_str(&mut self, string: &str, j: &Journal<A>) {
        self.vec.extend_from_slice(string.as_bytes(), j)
    }

    /// Returns this `String`'s capacity, in bytes.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```
    /// # use crndm::alloc::*;
    /// # use crndm::str::String;
    /// Heap::transaction(|j| {
    ///     let s = String::with_capacity(10, j);
    ///
    ///     assert!(s.capacity() >= 10);
    /// }).unwrap();
    /// ```
    #[inline]
    pub fn capacity(&self) -> usize {
        self.vec.capacity()
    }

    /// Ensures that this `String`'s capacity is at least `additional` bytes
    /// larger than its length.
    ///
    /// The capacity may be increased by more than `additional` bytes if it
    /// chooses, to prevent frequent reallocations.
    ///
    /// If you do not want this "at least" behavior, see the [`reserve_exact`]
    /// method.
    ///
    /// # Panics
    ///
    /// Panics if the new capacity overflows [`usize`].
    ///
    /// [`reserve_exact`]: struct.String.html#method.reserve_exact
    /// [`usize`]: ../../std/primitive.usize.html
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```
    /// # use crndm::alloc::*;
    /// # use crndm::str::String;
    /// Heap::transaction(|j| {
    ///     let mut s = String::<Heap>::new(j);
    ///
    ///     s.reserve(10, j);
    ///
    ///     assert!(s.capacity() >= 10);
    /// }).unwrap();
    /// ```
    ///
    /// This may not actually increase the capacity:
    ///
    /// ```
    /// # use crndm::alloc::*;
    /// # use crndm::str::String;
    /// Heap::transaction(|j| {
    ///     let mut s = String::with_capacity(10, j);
    ///     s.push('a', j);
    ///     s.push('b', j);
    ///
    ///     // s now has a length of 2 and a capacity of 10
    ///     assert_eq!(2, s.len());
    ///     assert_eq!(10, s.capacity());
    ///
    ///     // Since we already have an extra 8 capacity, calling this...
    ///     s.reserve(8, j);
    ///
    ///     // ... doesn't actually increase.
    ///     assert_eq!(10, s.capacity());
    /// }).unwrap();
    /// ```
    #[inline]
    pub fn reserve(&mut self, additional: usize, j: &Journal<A>) {
        self.vec.reserve(additional, j)
    }

    /// Shrinks the capacity of this `String` to match its length.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```
    /// # use crndm::alloc::*;
    /// # use crndm::str::String;
    /// # use crndm::convert::PFrom;
    /// crndm::transaction(|j| {
    ///     let mut s = String::<Heap>::pfrom("foo", j);
    ///
    ///     s.reserve(100, j);
    ///     assert!(s.capacity() >= 100);
    ///
    ///     s.shrink_to_fit(j);
    ///     assert_eq!(3, s.capacity());
    /// }).unwrap();
    /// ```
    #[inline]
    pub fn shrink_to_fit(&mut self, j: &Journal<A>) {
        self.vec.shrink_to_fit(j)
    }

    /// Shrinks the capacity of this `String` with a lower bound.
    ///
    /// The capacity will remain at least as large as both the length
    /// and the supplied value.
    ///
    /// Panics if the current capacity is smaller than the supplied
    /// minimum capacity.
    ///
    /// # Examples
    ///
    /// ```
    /// # use crndm::alloc::*;
    /// # use crndm::str::String;
    /// # use crndm::convert::PFrom;
    /// Heap::transaction(|j| {
    ///     let mut s = String::<Heap>::pfrom("foo", j);
    ///
    ///     s.reserve(100, j);
    ///     assert!(s.capacity() >= 100);
    ///
    ///     s.shrink_to(10, j);
    ///     assert!(s.capacity() >= 10);
    ///     s.shrink_to(0, j);
    ///     assert!(s.capacity() >= 3);
    /// }).unwrap();
    /// ```
    #[inline]
    pub fn shrink_to(&mut self, min_capacity: usize, j: &Journal<A>) {
        self.vec.shrink_to(min_capacity, j)
    }

    /// Appends the given [`char`] to the end of this `String`.
    ///
    /// [`char`]: ../../std/primitive.char.html
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```
    /// # use crndm::alloc::*;
    /// # use crndm::str::*;
    /// # use crndm::convert::PFrom;
    /// # Heap::transaction(|j| {
    /// let mut s = String::pfrom("abc", j);
    ///
    /// s.push('1', j);
    /// s.push('2', j);
    /// s.push('3', j);
    ///
    /// assert_eq!("abc123", s);
    /// # }).unwrap();
    /// ```
    #[inline]
    pub fn push(&mut self, ch: char, j: &Journal<A>) {
        match ch.len_utf8() {
            1 => self.vec.push(ch as u8, j),
            _ => self
                .vec
                .extend_from_slice(ch.encode_utf8(&mut [0; 4]).as_bytes(), j),
        }
    }

    /// Returns a byte slice of this `String`'s contents.
    ///
    /// The inverse of this method is [`from_utf8`].
    ///
    /// [`from_utf8`]: #method.from_utf8
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```
    /// # use crndm::alloc::*;
    /// # use crndm::str::String;
    /// # use crndm::convert::PFrom;
    /// Heap::transaction(|j| {
    ///     let s = String::<Heap>::pfrom("hello", j);
    ///     assert_eq!(&[104, 101, 108, 108, 111], s.as_bytes());
    /// }).unwrap();
    /// ```
    #[inline]
    pub fn as_bytes(&self) -> &[u8] {
        &self.vec
    }

    /// Shortens this `String` to the specified length.
    ///
    /// If `new_len` is greater than the string's current length, this has no
    /// effect.
    ///
    /// Note that this method has no effect on the allocated capacity
    /// of the string
    ///
    /// # Panics
    ///
    /// Panics if `new_len` does not lie on a [`char`] boundary.
    ///
    /// [`char`]: ../../std/primitive.char.html
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```
    /// # use crndm::alloc::*;
    /// # use crndm::str::String;
    /// # use crndm::convert::PFrom;
    /// Heap::transaction(|j| {
    ///     let mut s = String::<Heap>::pfrom("hello", j);
    ///
    ///     s.truncate(2);
    ///
    ///     assert_eq!("he", s);
    /// }).unwrap();
    /// ```
    #[inline]
    pub fn truncate(&mut self, new_len: usize) {
        if new_len <= self.len() {
            assert!(str::is_char_boundary(self.as_str(), new_len));
            self.vec.truncate(new_len)
        }
    }

    /// Removes the last character from the string buffer and returns it.
    ///
    /// Returns [`None`] if this `String` is empty.
    ///
    /// [`None`]: ../../std/option/enum.Option.html#variant.None
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```
    /// # use crndm::alloc::*;
    /// # use crndm::str::*;
    /// # use crndm::convert::PFrom;
    /// # Heap::transaction(|j| {
    /// let mut s = String::pfrom("foo", j);
    ///
    /// assert_eq!(s.pop(), Some('o'));
    /// assert_eq!(s.pop(), Some('o'));
    /// assert_eq!(s.pop(), Some('f'));
    ///
    /// assert_eq!(s.pop(), None);
    /// # }).unwrap();
    /// ```
    #[inline]
    pub fn pop(&mut self) -> Option<char> {
        if self.vec.is_empty() {
            None
        } else {
            let ch = self.as_str().chars().rev().next()?;
            let newlen = self.len() - ch.len_utf8();
            unsafe {
                self.vec.set_len(newlen);
            }
            Some(ch)
        }
    }

    /// Removes a [`char`] from this `String` at a byte position and returns it.
    ///
    /// This is an `O(n)` operation, as it requires copying every element in the
    /// buffer.
    ///
    /// # Panics
    ///
    /// Panics if `idx` is larger than or equal to the `String`'s length,
    /// or if it does not lie on a [`char`] boundary.
    ///
    /// [`char`]: ../../std/primitive.char.html
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```
    /// # use crndm::alloc::*;
    /// # use crndm::str::*;
    /// # use crndm::convert::PFrom;
    /// # Heap::transaction(|j| {
    /// let mut s = String::pfrom("foo", j);
    ///
    /// assert_eq!(s.remove(0), 'f');
    /// assert_eq!(s.remove(1), 'o');
    /// assert_eq!(s.remove(0), 'o');
    /// # }).unwrap();
    /// ```
    #[inline]
    pub fn remove(&mut self, idx: usize) -> char {
        let ch = match self.as_str()[idx..].chars().next() {
            Some(ch) => ch,
            None => panic!("cannot remove a char from the end of a string"),
        };

        let next = idx + ch.len_utf8();
        let len = self.len();
        unsafe {
            ptr::copy(
                self.vec.as_ptr().add(next),
                self.vec.as_slice_mut().as_mut_ptr().add(idx),
                len - next,
            );
            self.vec.set_len(len - (next - idx));
        }
        ch
    }

    /// Retains only the characters specified by the predicate.
    ///
    /// In other words, remove all characters `c` such that `f(c)` returns `false`.
    /// This method operates in place, visiting each character exactly once in the
    /// original order, and preserves the order of the retained characters.
    ///
    /// # Examples
    ///
    /// ```
    /// # use crndm::alloc::*;
    /// # use crndm::str::*;
    /// # use crndm::convert::PFrom;
    /// # Heap::transaction(|j| {
    /// let mut s = String::pfrom("f_o_ob_ar", j);
    ///
    /// s.retain(|c| c != '_');
    ///
    /// assert_eq!(s, "foobar");
    /// # }).unwrap();
    /// ```
    ///
    /// The exact order may be useful for tracking external state, like an index.
    ///
    /// ```
    /// # use crndm::alloc::*;
    /// # use crndm::str::*;
    /// # use crndm::convert::PFrom;
    /// # Heap::transaction(|j| {
    /// let mut s = String::pfrom("abcde", j);
    /// let keep = [false, true, true, false, true];
    /// let mut i = 0;
    /// s.retain(|_| (keep[i], i += 1).0);
    /// assert_eq!(s, "bce");
    /// # }).unwrap();
    /// ```
    #[inline]
    pub fn retain<F>(&mut self, mut f: F)
    where
        F: FnMut(char) -> bool,
    {
        let len = self.len();
        let mut del_bytes = 0;
        let mut idx = 0;

        while idx < len {
            let ch = unsafe {
                self.as_str()
                    .get_unchecked(idx..len)
                    .chars()
                    .next()
                    .unwrap()
            };
            let ch_len = ch.len_utf8();

            if !f(ch) {
                del_bytes += ch_len;
            } else if del_bytes > 0 {
                unsafe {
                    ptr::copy(
                        self.vec.as_ptr().add(idx),
                        self.vec.as_slice_mut().as_mut_ptr().add(idx - del_bytes),
                        ch_len,
                    );
                }
            }

            // Point idx to the next char
            idx += ch_len;
        }

        if del_bytes > 0 {
            unsafe {
                self.vec.set_len(len - del_bytes);
            }
        }
    }

    /// Inserts a character into this `String` at a byte position.
    ///
    /// This is an `O(n)` operation as it requires copying every element in the
    /// buffer.
    ///
    /// # Panics
    ///
    /// Panics if `idx` is larger than the `String`'s length, or if it does not
    /// lie on a [`char`] boundary.
    ///
    /// [`char`]: ../../std/primitive.char.html
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```
    /// let mut s = String::with_capacity(3);
    ///
    /// s.insert(0, 'f');
    /// s.insert(1, 'o');
    /// s.insert(2, 'o');
    ///
    /// assert_eq!("foo", s);
    /// ```
    #[inline]
    pub fn insert(&mut self, idx: usize, ch: char, j: &Journal<A>) {
        assert!(self.as_str().is_char_boundary(idx));
        let mut bits = [0; 4];
        let bits = ch.encode_utf8(&mut bits).as_bytes();

        unsafe {
            self.insert_bytes(idx, bits, j);
        }
    }

    unsafe fn insert_bytes(&mut self, idx: usize, bytes: &[u8], j: &Journal<A>) {
        let len = self.len();
        let amt = bytes.len();
        self.vec.reserve(amt, j);

        ptr::copy(
            self.vec.as_ptr().add(idx),
            self.vec.as_slice_mut().as_mut_ptr().add(idx + amt),
            len - idx,
        );
        ptr::copy(
            bytes.as_ptr(),
            self.vec.as_slice_mut().as_mut_ptr().add(idx),
            amt,
        );
        self.vec.set_len(len + amt);
    }

    /// Inserts a string slice into this `String` at a byte position.
    ///
    /// This is an `O(n)` operation as it requires copying every element in the
    /// buffer.
    ///
    /// # Panics
    ///
    /// Panics if `idx` is larger than the `String`'s length, or if it does not
    /// lie on a [`char`] boundary.
    ///
    /// [`char`]: ../../std/primitive.char.html
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```
    /// # use crndm::alloc::*;
    /// # use crndm::str::*;
    /// # use crndm::convert::PFrom;
    /// # Heap::transaction(|j| {
    /// let mut s = String::pfrom("bar", j);
    ///
    /// s.insert_str(0, "foo", j);
    ///
    /// assert_eq!("foobar", s);
    /// # }).unwrap();
    /// ```
    #[inline]
    pub fn insert_str(&mut self, idx: usize, string: &str, j: &Journal<A>) {
        assert!(self.as_str().is_char_boundary(idx));

        unsafe {
            self.insert_bytes(idx, string.as_bytes(), j);
        }
    }

    /// Returns a mutable reference to the contents of this `String`.
    ///
    /// # Safety
    ///
    /// This function is unsafe because it does not check that the bytes passed
    /// to it are valid UTF-8. If this constraint is violated, it may cause
    /// memory unsafety issues with future users of the `String`, as the rest of
    /// the standard library assumes that `String`s are valid UTF-8.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```
    /// # use crndm::alloc::*;
    /// # use crndm::convert::PFrom;
    /// # use crndm::str::*;
    /// # Heap::transaction(|j| {
    /// let mut s = String::pfrom("hello", j);
    ///
    /// unsafe {
    ///     let mut vec = s.as_mut_vec();
    ///     assert_eq!(&[104, 101, 108, 108, 111][..], &vec[..]);
    ///
    ///     vec.reverse();
    /// }
    /// assert_eq!(s, "olleh");
    /// # }).unwrap();
    /// ```
    #[inline]
    pub unsafe fn as_mut_vec(&mut self) -> &mut Vec<u8, A> {
        &mut self.vec
    }

    /// Returns the length of this `String`, in bytes, not [`char`]s or
    /// graphemes. In other words, it may not be what a human considers the
    /// length of the string.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```
    /// # use crndm::alloc::*;
    /// # use crndm::str::*;
    /// # use crndm::convert::PFrom;
    /// # Heap::transaction(|j| {
    /// let a = String::pfrom("foo", j);
    /// assert_eq!(a.len(), 3);
    ///
    /// let fancy_f = String::pfrom("ƒoo", j);
    /// assert_eq!(fancy_f.len(), 4);
    /// assert_eq!(fancy_f.chars().count(), 3);
    /// # }).unwrap();
    /// ```
    #[inline]
    pub fn len(&self) -> usize {
        self.vec.len()
    }

    /// Returns `true` if this `String` has a length of zero, and `false` otherwise.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```
    /// let mut v = String::new();
    /// assert!(v.is_empty());
    ///
    /// v.push('a');
    /// assert!(!v.is_empty());
    /// ```
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Splits the string into two at the given index.
    ///
    /// Returns a newly allocated `String`. `self` contains bytes `[0, at)`, and
    /// the returned `String` contains bytes `[at, len)`. `at` must be on the
    /// boundary of a UTF-8 code point.
    ///
    /// Note that the capacity of `self` does not change.
    ///
    /// # Panics
    ///
    /// Panics if `at` is not on a `UTF-8` code point boundary, or if it is beyond the last
    /// code point of the string.
    ///
    /// # Examples
    ///
    /// ```
    /// # use crndm::alloc::*;
    /// # use crndm::str::*;
    /// # use crndm::convert::PFrom;
    /// # Heap::transaction(|j| {
    /// let mut hello = String::pfrom("Hello, World!", j);
    /// let world = hello.split_off(7, j);
    /// assert_eq!(hello, "Hello, ");
    /// assert_eq!(world, "World!");
    /// # }).unwrap();
    /// ```
    #[inline]
    pub fn split_off(&mut self, at: usize, j: &Journal<A>) -> String<A> {
        assert!(self.as_str().is_char_boundary(at));
        let other = self.vec.split_off(at, j);
        unsafe { String::from_utf8_unchecked(other.as_slice().to_vec(), j) }
    }

    /// Truncates this `String`, removing all contents.
    ///
    /// While this means the `String` will have a length of zero, it does not
    /// touch its capacity.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```
    /// # use crndm::alloc::*;
    /// # use crndm::str::*;
    /// # use crndm::convert::PFrom;
    /// # Heap::transaction(|j| {
    /// let mut s = String::pfrom("foo", j);
    ///
    /// s.clear();
    ///
    /// assert!(s.is_empty());
    /// assert_eq!(0, s.len());
    /// assert_eq!(3, s.capacity());
    /// # }).unwrap();
    /// ```
    #[inline]
    pub fn clear(&mut self) {
        self.vec.clear()
    }

    /// Drops content without logging
    pub(crate) unsafe fn free_nolog(&mut self) {
        self.vec.free_nolog();
    }

    /// Removes the specified range in the string,
    /// and replaces it with the given string.
    /// The given string doesn't need to be the same length as the range.
    ///
    /// # Panics
    ///
    /// Panics if the starting point or end point do not lie on a [`char`]
    /// boundary, or if they're out of bounds.
    ///
    /// [`char`]: ../../std/primitive.char.html
    /// [`Vec::splice`]: ../../std/vec/struct.Vec.html#method.splice
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```
    /// # use crndm::alloc::*;
    /// # use crndm::str::*;
    /// # use crndm::convert::PFrom;
    /// # Heap::transaction(|j| {
    /// let mut s = String::pfrom("α is alpha, β is beta", j);
    /// let beta_offset = s.find('β').unwrap_or(s.len());
    ///
    /// // Replace the range up until the β from the string
    /// s.replace_range(..beta_offset, "Α is capital alpha; ", j);
    /// assert_eq!(s, "Α is capital alpha; β is beta");
    /// # }).unwrap();
    /// ```
    pub fn replace_range<R>(&mut self, range: R, replace_with: &str, j: &Journal<A>)
    where
        R: RangeBounds<usize>,
    {
        let mut s = self.as_str().to_string();
        s.replace_range(range, replace_with);
        if s.len() > self.len() {
            self.vec.reserve(s.len()-self.len(), j);
        }
        let slice: &mut [u8] = self.vec.as_mut();
        unsafe {
            ptr::copy_nonoverlapping(
                s.as_bytes() as *const _ as *const u8, 
                slice as *mut _ as *mut u8,
                s.len());
            self.vec.set_len(s.len());
        }
    }

    // /// Converts this `String` into a [`Box`]`<`[`str`]`>`.
    // ///
    // /// This will drop any excess capacity.
    // ///
    // /// [`Box`]: ../../std/boxed/struct.Box.html
    // /// [`str`]: ../../std/primitive.str.html
    // ///
    // /// # Examples
    // ///
    // /// Basic usage:
    // ///
    // /// ```
    // /// let s = String::pfrom("hello", j);
    // ///
    // /// let b = s.into_boxed_str();
    // /// ```
    // #[inline]
    // pub fn into_boxed_str(self) -> Box<str> {
    //     let slice = self.vec.into_boxed_slice();
    //     unsafe { from_boxed_utf8_unchecked(slice) }
    // }
}

impl<A: MemPool> PClone<A> for String<A> {
    fn pclone(&self, journal: &Journal<A>) -> Self {
        Self {
            vec: self.vec.pclone(journal),
        }
    }

    fn pclone_from(&mut self, source: &Self, journal: &Journal<A>) {
        self.vec.pclone_from(&source.vec, journal);
    }
}

// impl<A: MemPool> Clone for String<A> {
//     fn clone(&self) -> Self {
//         let journal = &Journal::try_current().expect("This function should be called only inside a transaction").0;
//         Self { vec: self.vec.pclone(journal) }
//     }

//     fn clone_from(&mut self, source: &Self) {
//         let journal = &Journal::try_current().expect("This function should be called only inside a transaction").0;
//         self.vec.clone_from(&source.vec, journal);
//     }
// }
// impl<A: MemPool> FromIterator<char> for String<A> {
//     fn from_iter<I: IntoIterator<Item = char>>(iter: I) -> String<A> {
//         let mut buf = String::new();
//         buf.extend(iter);
//         buf
//     }
// }
// impl<'a, A: MemPool> FromIterator<&'a char> for String<A> {
//     fn from_iter<I: IntoIterator<Item = &'a char>>(iter: I) -> String<A> {
//         let mut buf = String::new();
//         buf.extend(iter);
//         buf
//     }
// }
// impl<'a, A: MemPool> FromIterator<&'a str> for String<A> {
//     fn from_iter<I: IntoIterator<Item = &'a str>>(iter: I) -> String<A> {
//         let mut buf = String::new();
//         buf.extend(iter);
//         buf
//     }
// }
// impl<A: MemPool> FromIterator<String<A>> for String<A> {
//     fn from_iter<I: IntoIterator<Item = String<A>>>(iter: I) -> String<A> {
//         let mut iterator = iter.into_iter();

//         // Because we're iterating over `String`s, we can avoid at least
//         // one allocation by getting the first string from the iterator
//         // and appending to it all the subsequent strings.
//         match iterator.next() {
//             None => String::new(),
//             Some(mut buf) => {
//                 buf.extend(iterator);
//                 buf
//             }
//         }
//     }
// }
// impl<'a, A: MemPool> FromIterator<Cow<'a, str>> for String<A> {
//     fn from_iter<I: IntoIterator<Item = Cow<'a, str>>>(iter: I) -> String<A> {
//         let mut iterator = iter.into_iter();

//         // Because we're iterating over CoWs, we can (potentially) avoid at least
//         // one allocation by getting the first item and appending to it all the
//         // subsequent items.
//         match iterator.next() {
//             None => String::new(),
//             Some(cow) => {
//                 let mut buf = cow.into_owned();
//                 buf.extend(iterator);
//                 buf
//             }
//         }
//     }
// }
// impl<A: MemPool> Extend<char> for String<A> {
//     fn extend<I: IntoIterator<Item = char>>(&mut self, iter: I) {
//         let iterator = iter.into_iter();
//         let (lower_bound, _) = iterator.size_hint();
//         self.reserve(lower_bound);
//         iterator.for_each(move |c| self.push(c));
//     }
// }
// impl<'a, A: MemPool> Extend<&'a char> for String<A> {
//     fn extend<I: IntoIterator<Item = &'a char>>(&mut self, iter: I) {
//         self.extend(iter.into_iter().cloned());
//     }
// }
// impl<'a, A: MemPool> Extend<&'a str> for String<A> {
//     fn extend<I: IntoIterator<Item = &'a str>>(&mut self, iter: I) {
//         iter.into_iter().for_each(move |s| self.push_str(s));
//     }
// }
// impl<A: MemPool> Extend<String<A>> for String<A> {
//     fn extend<I: IntoIterator<Item = String<A>>>(&mut self, iter: I) {
//         iter.into_iter().for_each(move |s| self.push_str(&s));
//     }
// }
// impl<'a, A: MemPool> Extend<Cow<'a, str>> for String<A> {
//     fn extend<I: IntoIterator<Item = Cow<'a, str>>>(&mut self, iter: I) {
//         iter.into_iter().for_each(move |s| self.push_str(&s));
//     }
// }

/// A convenience impl that delegates to the impl for `&str`
impl<'a, 'b, A: MemPool> Pattern<'a> for &'b String<A> {
    type Searcher = <&'b str as Pattern<'a>>::Searcher;

    fn into_searcher(self, haystack: &'a str) -> <&'b str as Pattern<'a>>::Searcher {
        self.as_str()[..].into_searcher(haystack)
    }

    #[inline]
    fn is_contained_in(self, haystack: &'a str) -> bool {
        self.as_str()[..].is_contained_in(haystack)
    }

    #[inline]
    fn is_prefix_of(self, haystack: &'a str) -> bool {
        self.as_str()[..].is_prefix_of(haystack)
    }
}

impl<A: MemPool> PartialEq for String<A> {
    #[inline]
    fn eq(&self, other: &String<A>) -> bool {
        PartialEq::eq(&self.as_str()[..], &other.as_str()[..])
    }
    #[inline]
    fn ne(&self, other: &String<A>) -> bool {
        PartialEq::ne(&self[..], &other[..])
    }
}

macro_rules! impl_eq {
    ($lhs:ty, $rhs: ty) => {
        #[allow(unused_lifetimes)]
        impl<'a, 'b, A: MemPool> PartialEq<$rhs> for $lhs {
            #[inline]
            fn eq(&self, other: &$rhs) -> bool {
                PartialEq::eq(&self[..], &other[..])
            }
            #[inline]
            fn ne(&self, other: &$rhs) -> bool {
                PartialEq::ne(&self[..], &other[..])
            }
        }
        #[allow(unused_lifetimes)]
        impl<'a, 'b, A: MemPool> PartialEq<$lhs> for $rhs {
            #[inline]
            fn eq(&self, other: &$lhs) -> bool {
                PartialEq::eq(&self[..], &other[..])
            }
            #[inline]
            fn ne(&self, other: &$lhs) -> bool {
                PartialEq::ne(&self[..], &other[..])
            }
        }
    };
}

impl_eq! { String<A>, str }
impl_eq! { String<A>, &'a str }
// impl_eq! { Cow<'a, str>, str }
// impl_eq! { Cow<'a, str>, &'b str }
// impl_eq! { Cow<'a, str>, String<A> }

impl<A: MemPool> Default for String<A> {
    /// Creates an empty `String`.
    #[inline]
    fn default() -> String<A> {
        String { vec: Vec::default() }
    }
}

impl<A: MemPool> fmt::Display for String<A> {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&**self, f)
    }
}
impl<A: MemPool> fmt::Debug for String<A> {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&**self, f)
    }
}
impl<A: MemPool> hash::Hash for String<A> {
    #[inline]
    fn hash<H: hash::Hasher>(&self, hasher: &mut H) {
        (**self).hash(hasher)
    }
}

// /// Implements the `+` operator for concatenating two strings.
// ///
// /// This consumes the `String` on the left-hand side and re-uses its buffer (growing it if
// /// necessary). This is done to avoid allocating a new `String` and copying the entire contents on
// /// every operation, which would lead to `O(n^2)` running time when building an `n`-byte string by
// /// repeated concatenation.
// ///
// /// The string on the right-hand side is only borrowed; its contents are copied into the returned
// /// `String`.
// ///
// /// # Examples
// ///
// /// Concatenating two `String`s takes the first by value and borrows the second:
// ///
// /// ```
// /// let a = String::pfrom("hello", j);
// /// let b = String::pfrom(" world", j);
// /// let c = a + &b;
// /// // `a` is moved and can no longer be used here.
// /// ```
// ///
// /// If you want to keep using the first `String`, you can clone it and append to the clone instead:
// ///
// /// ```
// /// let a = String::pfrom("hello", j);
// /// let b = String::pfrom(" world", j);
// /// let c = a.clone() + &b;
// /// // `a` is still valid here.
// /// ```
// ///
// /// Concatenating `&str` slices can be done by converting the first to a `String`:
// ///
// /// ```
// /// let a = "hello";
// /// let b = " world";
// /// let c = a.to_string() + b;
// /// ```
// impl<A: MemPool> Add<&str> for String<A> {
//     type Output = String<A>;

//     #[inline]
//     fn add(mut self, other: &str) -> String<A> {
//         self.push_str(other);
//         self
//     }
// }

// /// Implements the `+=` operator for appending to a `String`.
// ///
// /// This has the same behavior as the [`push_str`][String::push_str] method.
// impl<A: MemPool> AddAssign<&str> for String<A> {
//     #[inline]
//     fn add_assign(&mut self, other: &str) {
//         self.push_str(other);
//     }
// }

impl<A: MemPool> ops::Index<ops::Range<usize>> for String<A> {
    type Output = str;

    #[inline]
    fn index(&self, index: ops::Range<usize>) -> &str {
        &self[..][index]
    }
}

impl<A: MemPool> ops::Index<ops::RangeTo<usize>> for String<A> {
    type Output = str;

    #[inline]
    fn index(&self, index: ops::RangeTo<usize>) -> &str {
        &self[..][index]
    }
}

impl<A: MemPool> ops::Index<ops::RangeFrom<usize>> for String<A> {
    type Output = str;

    #[inline]
    fn index(&self, index: ops::RangeFrom<usize>) -> &str {
        &self[..][index]
    }
}

impl<A: MemPool> ops::Index<ops::RangeFull> for String<A> {
    type Output = str;

    #[inline]
    fn index(&self, _index: ops::RangeFull) -> &str {
        unsafe { str::from_utf8_unchecked(&self.vec) }
    }
}

impl<A: MemPool> ops::Index<ops::RangeInclusive<usize>> for String<A> {
    type Output = str;

    #[inline]
    fn index(&self, index: ops::RangeInclusive<usize>) -> &str {
        Index::index(&**self, index)
    }
}

impl<A: MemPool> ops::Index<ops::RangeToInclusive<usize>> for String<A> {
    type Output = str;

    #[inline]
    fn index(&self, index: ops::RangeToInclusive<usize>) -> &str {
        Index::index(&**self, index)
    }
}
impl<A: MemPool> ops::IndexMut<ops::Range<usize>> for String<A> {
    #[inline]
    fn index_mut(&mut self, index: ops::Range<usize>) -> &mut str {
        &mut self[..][index]
    }
}

impl<A: MemPool> ops::IndexMut<ops::RangeTo<usize>> for String<A> {
    #[inline]
    fn index_mut(&mut self, index: ops::RangeTo<usize>) -> &mut str {
        &mut self[..][index]
    }
}

impl<A: MemPool> ops::IndexMut<ops::RangeFrom<usize>> for String<A> {
    #[inline]
    fn index_mut(&mut self, index: ops::RangeFrom<usize>) -> &mut str {
        &mut self[..][index]
    }
}

impl<A: MemPool> ops::IndexMut<ops::RangeFull> for String<A> {
    #[inline]
    fn index_mut(&mut self, _index: ops::RangeFull) -> &mut str {
        unsafe { str::from_utf8_unchecked_mut(self.vec.as_slice_mut()) }
    }
}

impl<A: MemPool> ops::IndexMut<ops::RangeInclusive<usize>> for String<A> {
    #[inline]
    fn index_mut(&mut self, index: ops::RangeInclusive<usize>) -> &mut str {
        IndexMut::index_mut(&mut **self, index)
    }
}

impl<A: MemPool> ops::IndexMut<ops::RangeToInclusive<usize>> for String<A> {
    #[inline]
    fn index_mut(&mut self, index: ops::RangeToInclusive<usize>) -> &mut str {
        IndexMut::index_mut(&mut **self, index)
    }
}

impl<A: MemPool> ops::Deref for String<A> {
    type Target = str;

    #[inline]
    fn deref(&self) -> &str {
        unsafe { str::from_utf8_unchecked(&self.vec) }
    }
}

impl<A: MemPool> ops::DerefMut for String<A> {
    #[inline]
    fn deref_mut(&mut self) -> &mut str {
        unsafe { str::from_utf8_unchecked_mut(self.vec.as_slice_mut()) }
    }
}

pub trait ToStringSlice<A: MemPool> {
    /// Converts the given value to a `String`.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```
    /// # use crndm::alloc::*;
    /// # use crndm::str::*;
    /// # use crndm::convert::PFrom;
    /// Heap::transaction(|j| {
    ///     let i: Vec<i32> = vec![1,2,3];
    ///     let list = vec![
    ///         String::<Heap>::pfrom("1", j),
    ///         String::<Heap>::pfrom("2", j),
    ///         String::<Heap>::pfrom("3", j)
    ///     ];
    ///
    ///     assert_eq!(list, i.to_pstring(j));
    /// }).unwrap();
    /// ```
    fn to_pstring(&self, journal: &Journal<A>) -> StdVec<String<A>>;
}

impl<T: fmt::Display, A: MemPool> ToStringSlice<A> for [T] {
    #[inline]
    default fn to_pstring(&self, journal: &Journal<A>) -> StdVec<String<A>> {
        let mut vec = StdVec::<String<A>>::with_capacity(self.len());
        for v in self {
            use fmt::Write;
            let mut buf = StdString::new();
            buf.write_fmt(format_args!("{}", v))
                .expect("a Display implementation returned an error unexpectedly");
            buf.shrink_to_fit();
            vec.push(String::from_str(&buf, journal));
        }
        vec
    }
}

impl<A: MemPool> ToStringSlice<A> for StdVec<&str> {
    #[inline]
    default fn to_pstring(&self, journal: &Journal<A>) -> StdVec<String<A>> {
        let mut vec = StdVec::<String<A>>::with_capacity(self.len());
        for buf in self {
            vec.push(String::from_str(buf, journal));
        }
        vec
    }
}

pub trait ToString<A: MemPool> {
    /// Converts the given value to a `String`.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```
    /// # use crndm::alloc::*;
    /// # use crndm::convert::PFrom;
    /// # use crndm::str::*;
    /// Heap::transaction(|j| {
    ///     let i: i32 = 5;
    ///     let five = String::<Heap>::pfrom("5", j);
    ///
    ///     assert_eq!(five, i.to_pstring(j));
    /// }).unwrap();
    /// ```
    fn to_pstring(&self, journal: &Journal<A>) -> String<A>;
}

/// # Panics
///
/// In this implementation, the `to_string` method panics
/// if the `Display` implementation returns an error.
/// This indicates an incorrect `Display` implementation
/// since `fmt::Write for String` never returns an error itself.
impl<T: fmt::Display + ?Sized, A: MemPool> ToString<A> for T {
    #[inline]
    default fn to_pstring(&self, journal: &Journal<A>) -> String<A> {
        use fmt::Write;
        let mut buf = StdString::new();
        buf.write_fmt(format_args!("{}", self))
            .expect("a Display implementation returned an error unexpectedly");
        buf.shrink_to_fit();
        String::from_str(&buf, journal)
    }
}

impl<A: MemPool> ToString<A> for str {
    #[inline]
    fn to_pstring(&self, journal: &Journal<A>) -> String<A> {
        String::from_str(self, journal)
    }
}

impl<A: MemPool> ToString<A> for Cow<'_, str> {
    #[inline]
    fn to_pstring(&self, journal: &Journal<A>) -> String<A> {
        String::from_str(&self[..].to_owned(), journal)
    }
}

impl<A: MemPool> ToString<A> for StdString {
    #[inline]
    fn to_pstring(&self, journal: &Journal<A>) -> String<A> {
        String::from_str(self, journal)
    }
}

// impl<A: MemPool> ToString for String<A> {
//     #[inline]
//     fn to_string(&self) -> String<A> {
//         String::from(self)
//     }
// }
// impl<A: MemPool> ToString for Cow<'_, str> {
//     #[inline]
//     fn to_string(&self) -> String<A> {
//         self[..].to_owned()
//     }
// }
impl<A: MemPool> StdToString for String<A> {
    #[inline]
    fn to_string(&self) -> StdString {
        self.as_str().to_owned()
    }
}
impl<A: MemPool> AsRef<str> for String<A> {
    #[inline]
    fn as_ref(&self) -> &str {
        self
    }
}
impl<A: MemPool> AsMut<str> for String<A> {
    #[inline]
    fn as_mut(&mut self) -> &mut str {
        self
    }
}
impl<A: MemPool> AsRef<[u8]> for String<A> {
    #[inline]
    fn as_ref(&self) -> &[u8] {
        self.as_bytes()
    }
}
impl<A: MemPool> PFrom<&str, A> for String<A> {
    #[inline]
    fn pfrom(s: &str, j: &Journal<A>) -> String<A> {
        Self::from_str(s, j)
    }
}
// impl<A: MemPool> From<&mut str> for String<A> {
//     /// Converts a `&mut str` into a `String`.
//     ///
//     /// The result is allocated on the heap.
//     #[inline]
//     fn from(s: &mut str) -> String<A> {
//         s.to_owned()
//     }
// }
// impl<A: MemPool> From<&String<A>> for String<A> {
//     #[inline]
//     fn from(s: &String<A>) -> String<A> {
//         s.clone()
//     }
// }

// // note: test pulls in libstd, which causes errors here
// #[cfg(not(test))]
// impl<A: MemPool> From<Box<str>> for String<A> {
//     /// Converts the given boxed `str` slice to a `String`.
//     /// It is notable that the `str` slice is owned.
//     ///
//     /// # Examples
//     ///
//     /// Basic usage:
//     ///
//     /// ```
//     /// let s1: String<A> = String::pfrom("hello world", j);
//     /// let s2: Box<str> = s1.into_boxed_str();
//     /// let s3: String<A> = String::from(s2);
//     ///
//     /// assert_eq!("hello world", s3)
//     /// ```
//     fn from(s: Box<str>) -> String<A> {
//         s.into_string()
//     }
// }
// impl<A: MemPool> From<String<A>> for Box<str> {
//     /// Converts the given `String` to a boxed `str` slice that is owned.
//     ///
//     /// # Examples
//     ///
//     /// Basic usage:
//     ///
//     /// ```
//     /// let s1: String<A> = String::pfrom("hello world", j);
//     /// let s2: Box<str> = Box::from(s1);
//     /// let s3: String<A> = String::from(s2);
//     ///
//     /// assert_eq!("hello world", s3)
//     /// ```
//     fn from(s: String<A>) -> Box<str> {
//         s.into_boxed_str()
//     }
// }
// impl<'a, A: MemPool> From<Cow<'a, str>> for String<A> {
//     fn from(s: Cow<'a, str>) -> String<A> {
//         s.into_owned()
//     }
// }
impl<'a, A: MemPool> From<&'a String<A>> for Cow<'a, str> {
    #[inline]
    fn from(s: &'a String<A>) -> Cow<'a, str> {
        Cow::Borrowed(s)
    }
}
// impl<'a, A: MemPool> From<String<A>> for Cow<'a, String<A>> {
//     #[inline]
//     fn from(s: String<A>) -> Cow<'a, String<A>> {
//         Cow::Owned(s)
//     }
// }
// impl<'a, A: MemPool> From<&'a String<A>> for Cow<'a, String<A>> {
//     #[inline]
//     fn from(s: &'a String<A>) -> Cow<'a, String<A>> {
//         Cow::Borrowed(s)
//     }
// }
// impl<'a, A: MemPool> FromIterator<char> for Cow<'a, String<A>> {
//     fn from_iter<I: IntoIterator<Item = char>>(it: I) -> Cow<'a, String<A>> {
//         Cow::Owned(FromIterator::from_iter(it))
//     }
// }
// impl<'a, 'b, A: MemPool> FromIterator<&'b str> for Cow<'a, String<A>> {
//     fn from_iter<I: IntoIterator<Item = &'b str>>(it: I) -> Cow<'a, String<A>> {
//         Cow::Owned(FromIterator::from_iter(it))
//     }
// }
// impl<'a, A: MemPool> FromIterator<String<A>> for Cow<'a, String<A>> {
//     fn from_iter<I: IntoIterator<Item = String<A>>>(it: I) -> Cow<'a, String<A>> {
//         Cow::Owned(FromIterator::from_iter(it))
//     }
// }
impl<A: MemPool> From<String<A>> for Vec<u8, A> {
    /// Converts the given `String` to a vector `Vec` that holds values of type `u8`.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```
    /// # use crndm::vec::Vec;
    /// # use crndm::alloc::*;
    /// # use crndm::convert::PFrom;
    /// # use crndm::str::*;
    /// Heap::transaction(|j| {
    ///     let s1 = String::<Heap>::pfrom("hello world", j);
    ///     let v1 = Vec::from(s1);
    ///
    ///     for b in v1.as_slice() {
    ///         println!("{}", b);
    ///     }
    /// }).unwrap();
    /// ```
    fn from(string: String<A>) -> Vec<u8, A> {
        string.into_bytes()
    }
}
impl<A: MemPool> fmt::Write for String<A> {
    #[inline]
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let j = &Journal::try_current()
            .expect("This function should be called only inside a transaction")
            .0;
        self.push_str(s, j);
        Ok(())
    }

    #[inline]
    fn write_char(&mut self, c: char) -> fmt::Result {
        let j = &Journal::try_current()
            .expect("This function should be called only inside a transaction")
            .0;
        self.push(c, j);
        Ok(())
    }
}

impl<A: MemPool> RootObj<A> for String<A> {
    fn init(j: &Journal<A>) -> Self {
        Self::new(j)
    }
}

// /// A draining iterator for `String`.
// ///
// /// This struct is created by the [`drain`] method on [`String`]. See its
// /// documentation for more.
// ///
// /// [`drain`]: struct.String.html#method.drain
// /// [`String`]: struct.String.html
// pub struct Drain<'a, A: MemPool> {
//     /// Will be used as &'a mut String<A> in the destructor
//     string: *mut String<A>,
//     /// Start of part to remove
//     start: usize,
//     /// End of part to remove
//     end: usize,
//     /// Current remaining range to remove
//     iter: Chars<'a>,
// }
// impl<A: MemPool> fmt::Debug for Drain<'_, A> {
//     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
//         f.pad("Drain { .. }")
//     }
// }
// unsafe impl<A: MemPool> Sync for Drain<'_, A> {}
// unsafe impl<A: MemPool> Send for Drain<'_, A> {}
// impl<A: MemPool> Drop for Drain<'_, A> {
//     fn drop(&mut self) {
//         unsafe {
//             // Use Vec::drain. "Reaffirm" the bounds checks to avoid
//             // panic code being inserted again.
//             let self_vec = (*self.string).as_mut_vec();
//             if self.start <= self.end && self.end <= self_vec.len() {
//                 self_vec.drain(self.start..self.end);
//             }
//         }
//     }
// }
// impl<A: MemPool> Iterator for Drain<'_, A> {
//     type Item = char;

//     #[inline]
//     fn next(&mut self) -> Option<char> {
//         self.iter.next()
//     }

//     fn size_hint(&self) -> (usize, Option<usize>) {
//         self.iter.size_hint()
//     }

//     #[inline]
//     fn last(mut self) -> Option<char> {
//         self.next_back()
//     }
// }
// impl<A: MemPool> DoubleEndedIterator for Drain<'_, A> {
//     #[inline]
//     fn next_back(&mut self) -> Option<char> {
//         self.iter.next_back()
//     }
// }
// impl<A: MemPool> FusedIterator for Drain<'_, A> {}

#[cfg(test)]
mod test {
    use crate::default::*;
    use crate::boxed::Pbox;
    use crate::cell::*;
    use crate::str::*;

    type A = BuddyAlloc;

    #[test]
    fn test_pstring() {
        let root = A::open::<Pbox<LogRefCell<String<A>, A>, A>>("sb6.pool", O_CFNE).unwrap();

        // let hello = "Hello World!!!";
        let _ = A::transaction(|j| {
            println!("RootObj = {}", root);
            let mut root = root.borrow_mut(j);
            // std::process::exit(1);
            *root = 5129483505_u64.to_pstring(j);
            println!("RootObj = {}", root);
            // panic!("test");
            // *root = String::<A>::pfrom("test", j);
        });
        println!("Usage = {}", A::used());
    }

    #[test]
    fn test_list_pstring() {}
}
