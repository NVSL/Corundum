//! Single-threaded reference-counting persistent pointers

use std::panic::RefUnwindSafe;
use std::panic::UnwindSafe;
use crate::alloc::{MemPool, PmemUsage};
use crate::cell::VCell;
use crate::clone::*;
use crate::ptr::Ptr;
use crate::stm::*;
use crate::ll::*;
use crate::*;
use std::cmp::Ordering;
use std::hash::Hash;
use std::hash::Hasher;
use std::marker::PhantomData;
use std::mem::MaybeUninit;
use std::ops::Deref;
use std::*;

#[derive(Debug)]
struct Counter {
    strong: usize,
    weak: usize,

    #[cfg(not(feature = "no_log_rc"))]
    has_log: u8,
}

pub struct PrcBox<T: ?Sized, A: MemPool> {
    counter: Counter,

    #[cfg(not(feature = "no_volatile_pointers"))]
    vlist: VCell<VWeakList, A>,

    dummy: [A; 0],
    value: T,
}

unsafe impl<T: ?Sized, A: MemPool> PSafe for PrcBox<T, A> {}
unsafe impl<T: ?Sized, A: MemPool> TxInSafe for PrcBox<T, A> {}
impl<T: ?Sized, A: MemPool> UnwindSafe for PrcBox<T, A> {}
impl<T: ?Sized, A: MemPool> RefUnwindSafe for PrcBox<T, A> {}
impl<T: ?Sized, A: MemPool> !VSafe for PrcBox<T, A> {}
impl<T: ?Sized, A: MemPool> !PSend for PrcBox<T, A> {}

unsafe fn set_data_ptr<T: ?Sized, U>(mut ptr: *mut T, data: *mut U) -> *mut T {
    std::ptr::write(&mut ptr as *mut _ as *mut *mut u8, data as *mut u8);
    ptr
}

/// A single-thread reference-counting persistent pointer. 'Prc' stands for
/// 'Persistent Reference Counted'.
///
/// The main aspect of `Prc<T>` is that its counters are transactional which
/// means that functions [`pclone`], [`downgrade`], and [`upgrade`] require a
/// [`Journal`] to operate. In other words, you need to wrap them in a
/// [`transaction`].
/// 
/// `Prc` uses reference counting to manage memory. Although it provides a fast
/// solution for deallocation without scan, cyclic references yield a memory
/// leak. At this point, we have not provided a static solution to detect cyclic
/// references. However, following Rust's partial solution for that, you may use
/// [`Weak`] references for reference cycles.
/// 
/// References to data can be strong (using [`pclone`]), weak (using [`downgrade`]),
/// or demote weak (using [`demote`]). The first two generate NV-to-NV
/// pointers, while the last on is a V-to-NV pointer. Please see [`Weak`] and
/// [`VWeak`] for more details on their implementation and safety.
///
/// # Examples
///
/// ```
/// # use corundum::alloc::*;
/// # type P = Heap;
/// use corundum::prc::Prc;
/// use corundum::clone::PClone;
/// 
/// # #[allow(unused)]
/// P::transaction(|j| {
///     let p = Prc::<i32,P>::new(1, j);
/// 
///     // Create a new persistent strong reference
///     let s = p.pclone(j);
/// 
///     assert_eq!(*p, *s);
///     assert_eq!(2, Prc::strong_count(&p));
///     assert_eq!(0, Prc::weak_count(&p));
/// 
///     // Create a new persistent weak reference
///     let w = Prc::downgrade(&p, j);
///     assert_eq!(2, Prc::strong_count(&p));
///     assert_eq!(1, Prc::weak_count(&p));
/// 
///     // Create a new volatile weak reference
///     let v = Prc::demote(&p);
///     assert_eq!(2, Prc::strong_count(&p));
///     assert_eq!(1, Prc::weak_count(&p));
/// 
///     // Upgrade the persistent weak ref to a strong ref
///     let ws = w.upgrade(j).unwrap();
///     assert_eq!(3, Prc::strong_count(&p));
///     assert_eq!(1, Prc::weak_count(&p));
/// 
///     // Upgrade the demote weak ref to a strong ref
///     let vs = w.upgrade(j).unwrap();
///     assert_eq!(4, Prc::strong_count(&p));
///     assert_eq!(1, Prc::weak_count(&p));
/// }).unwrap();
/// ```
/// 
/// [`pclone`]: #method.pclone
/// [`downgrade`]: #method.downgrade
/// [`demote`]: #method.demote
/// [`upgrade`]: ./struct.Weak.html#method.upgrade
/// [`promote`]: ./struct.Weak.html#method.promote
/// [`Journal`]: ../stm/journal/struct.Journal.html
/// [`transaction`]: ../stm/fn.transaction.html
pub struct Prc<T: PSafe + ?Sized, A: MemPool> {
    ptr: Ptr<PrcBox<T, A>, A>,
    phantom: PhantomData<T>,
}

impl<T: ?Sized, A: MemPool> !TxOutSafe for Prc<T, A> {}
impl<T: ?Sized, A: MemPool> !Send for Prc<T, A> {}
impl<T: ?Sized, A: MemPool> !Sync for Prc<T, A> {}
impl<T: ?Sized, A: MemPool> !VSafe for Prc<T, A> {}
impl<T: ?Sized, A: MemPool> !PSend for Prc<T, A> {}

impl<T: PSafe, A: MemPool> Prc<T, A> {
    /// Constructs a new `Prc<T>`.
    ///
    /// It also creates a `DropOnFailure` log to make sure that if the program
    /// crashes, the allocation drops of recovery.
    ///
    /// # Examples
    ///
    /// ```
    /// # use corundum::alloc::*;
    /// # type P = Heap;
    /// use corundum::prc::Prc;
    ///
    /// P::transaction(|j| {
    ///     let five = Prc::new(5, j);
    /// }).unwrap();
    /// ```
    pub fn new(value: T, journal: &Journal<A>) -> Prc<T, A> {
        unsafe {
            let ptr = Ptr::new_unchecked(A::new(
                PrcBox::<T, A> {
                    counter: Counter {
                        strong: 1,
                        weak: 1,

                        #[cfg(not(feature = "no_log_rc"))]
                        has_log: 0,
                    },

                    #[cfg(not(feature = "no_volatile_pointers"))]
                    vlist: VCell::new(VWeakList::default()),

                    dummy: [],
                    value,
                },
                journal,
            ));
            Self::from_inner(ptr)
        }
    }

    /// Constructs a new `Prc` with uninitialized contents.
    ///
    /// A `DropOnFailure` log is taken for the allocation.
    ///
    /// # Examples
    ///
    /// ```
    /// # use corundum::alloc::*;
    /// # type P = Heap;
    /// use corundum::prc::Prc;
    ///
    /// P::transaction(|j| {
    ///     let mut five = Prc::<u32,Heap>::new_uninit(j);
    ///
    ///     let five = unsafe {
    ///         // Deferred initialization:
    ///         Prc::get_mut_unchecked(&mut five).as_mut_ptr().write(5);
    ///
    ///         five.assume_init()
    ///     };
    ///
    ///     assert_eq!(*five, 5)
    /// }).unwrap();
    /// ```
    pub fn new_uninit(journal: &Journal<A>) -> Prc<MaybeUninit<T>, A> {
        unsafe {
            Prc::from_inner(Ptr::from_mut(A::new(
                PrcBox {
                    counter: Counter {
                        strong: 1,
                        weak: 1,

                        #[cfg(not(feature = "no_log_rc"))]
                        has_log: 0,
                    },

                    #[cfg(not(feature = "no_volatile_pointers"))]
                    vlist: VCell::new(VWeakList::default()),

                    dummy: [],
                    value: MaybeUninit::<T>::uninit(),
                },
                journal,
            )))
        }
    }

    /// Constructs a new `Prc` with uninitialized contents, with the memory
    /// being filled with `0` bytes.
    ///
    /// See `MaybeUninit::zeroed` for examples of correct and incorrect usage of
    /// this method.
    ///
    /// # Examples
    ///
    /// ```
    /// # use corundum::alloc::*;
    /// # type P = Heap;
    /// use corundum::prc::Prc;
    ///
    /// P::transaction(|j| {
    ///     let zero = Prc::<i32,P>::new_zeroed(j);
    ///     let zero = unsafe { zero.assume_init() };
    ///
    ///     assert_eq!(*zero, 0)
    /// }).unwrap();
    /// ```
    ///
    pub fn new_zeroed(journal: &Journal<A>) -> Prc<mem::MaybeUninit<T>, A> {
        unsafe {
            let mut uninit = Self::new_uninit(journal);
            std::ptr::write_bytes::<T>(Prc::get_mut_unchecked(&mut uninit).as_mut_ptr(), 0, 1);
            uninit
        }
    }

    /// Owns contents of `p` without cloning, leaving `p` untouched
    pub fn from(p: Prc<T, A>) -> Self {
        let res = Self::from_inner(p.ptr);
        mem::forget(p);
        res
    }
}

impl<T: PSafe + ?Sized, A: MemPool> Prc<T, A> {
    #[inline]
    fn from_inner(ptr: Ptr<PrcBox<T, A>, A>) -> Self {
        Prc {
            ptr,
            phantom: PhantomData,
        }
    }

    #[inline(always)]
    fn inner(&self) -> &PrcBox<T, A> {
        self.ptr.as_ref()
    }

    #[allow(clippy::missing_safety_doc)]
    unsafe fn from_ptr(ptr: *mut PrcBox<T, A>, j: &Journal<A>) -> Self {
        let off = A::off_unchecked(ptr);
        let res = Self::from_inner(Ptr::from_off_unchecked(off));
        res.inc_strong(j);
        res
    }
}

impl<T: PSafe, A: MemPool> Prc<mem::MaybeUninit<T>, A> {
    /// Converts to `Rc<T>`.
    ///
    /// # Safety
    ///
    /// As with [`MaybeUninit::assume_init`],
    /// it is up to the caller to guarantee that the inner value
    /// really is in an initialized state.
    /// Calling this when the content is not yet fully initialized
    /// causes immediate undefined behavior.
    ///
    /// [`MaybeUninit::assume_init`]: ../../std/mem/union.MaybeUninit.html#method.assume_init
    ///
    /// # Examples
    ///
    /// ```
    /// # use corundum::alloc::*;
    /// # type P = Heap;
    /// use corundum::prc::Prc;
    ///
    /// corundum::transaction(|j| {
    ///     let mut five = Prc::<u32,P>::new_uninit(j);
    ///
    ///     let five = unsafe {
    ///         // Deferred initialization:
    ///         Prc::get_mut_unchecked(&mut five).as_mut_ptr().write(5);
    ///
    ///         five.assume_init()
    ///     };
    ///
    ///     assert_eq!(*five, 5);
    /// }).unwrap();
    /// ```
    #[inline]
    pub unsafe fn assume_init(self) -> Prc<T, A> {
        Prc::from_inner(mem::ManuallyDrop::new(self).ptr.cast())
    }
}

impl<T: PSafe, A: MemPool> Prc<MaybeUninit<T>, A> {
    #[inline]

    /// Returns a mutable reference into the given `Prc`, if there are
    /// no other `Prc` or `Weak` pointers to the same allocation.
    ///
    /// Returns `None` otherwise, because it is not safe to mutate a shared
    /// value. It only works for `Prc<MaybeUninit<T>>` to be able to defer the
    /// initialization.
    ///
    /// # Examples
    ///
    /// ```
    /// # use corundum::alloc::*;
    /// # type P = Heap;
    /// use corundum::prc::Prc;
    ///
    /// P::transaction(|j| {
    ///     let mut five = Prc::<u32,P>::new_uninit(j);
    ///
    ///     let five = unsafe {
    ///         // Deferred initialization:
    ///         Prc::get_mut(&mut five).unwrap().as_mut_ptr().write(5);
    ///
    ///         five.assume_init()
    ///     };
    ///
    ///     assert_eq!(*five, 5)
    /// }).unwrap();
    /// ```
    pub fn get_mut(this: &mut Self) -> Option<&mut MaybeUninit<T>> {
        if Prc::is_unique(this) {
            unsafe { Some(Prc::get_mut_unchecked(this)) }
        } else {
            None
        }
    }

    #[inline]
    /// Returns a mutable reference into the given `Prc`,
    /// without any check.
    ///
    /// It only works for `Prc<MaybeUninit<T>>` to be able to defer the
    /// initialization.
    ///
    /// # Safety
    ///
    /// Any other `Prc` or `Weak` pointers to the same allocation must not be
    /// dereferenced for the duration of the returned borrow.
    /// This is trivially the case if no such pointers exist,
    /// for example immediately after `Rc::new`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use corundum::alloc::*;
    /// # type P = Heap;
    /// use corundum::prc::Prc;
    ///
    /// P::transaction(|j| {
    ///     let mut five = Prc::<i32,P>::new_uninit(j);
    ///
    ///     let five = unsafe {
    ///         // Deferred initialization:
    ///         Prc::get_mut_unchecked(&mut five).as_mut_ptr().write(5);
    ///
    ///         five.assume_init()
    ///     };
    ///
    ///     assert_eq!(*five, 5);
    /// }).unwrap();
    /// ```
    pub unsafe fn get_mut_unchecked(this: &mut Self) -> &mut MaybeUninit<T> {
        &mut this.ptr.value
    }
}

impl<T: PSafe + ?Sized, A: MemPool> Prc<T, A> {
    /// Creates a new `Weak` persistent pointer to this allocation.
    /// 
    /// The `Weak` pointer can later be [`upgrade`]d to a `Prc`.
    ///
    /// [`upgrade`]: ./struct.Weak.html#upgrade
    /// 
    /// # Examples
    ///
    /// ```
    /// # use corundum::alloc::*;
    /// # type P = Heap;
    /// use corundum::prc::Prc;
    ///
    /// P::transaction(|j| {
    ///     let five = Prc::new(5, j);
    ///     let _weak_five = Prc::downgrade(&five, j);
    /// }).unwrap()
    /// ```
    pub fn downgrade(this: &Self, journal: &Journal<A>) -> Weak<T, A> {
        this.inc_weak(journal);
        debug_assert!(!this.ptr.is_dangling());
        Weak { ptr: this.ptr }
    }

    /// Creates a new `Weak` volatile to this allocation.
    /// 
    /// The `Weak` pointer can later be [`promote`]d to a `Prc`.
    ///
    /// [`promote`]: ./struct.VWeak.html#upgrade
    /// 
    /// # Examples
    ///
    /// ```
    /// # use corundum::alloc::*;
    /// # type P = Heap;
    /// use corundum::prc::Prc;
    ///
    /// P::transaction(|j| {
    ///     let five = Prc::new(5, j);
    ///     let weak_five = Prc::demote(&five);
    /// 
    ///     assert_eq!(Prc::strong_count(&five), 1);
    /// 
    ///     if let Some(f) = weak_five.promote(j) {
    ///         assert_eq!(*f, 5);
    ///         assert_eq!(Prc::strong_count(&five), 2);
    ///     }
    /// 
    ///     assert_eq!(Prc::strong_count(&five), 1);
    /// }).unwrap()
    /// ```
    pub fn demote(this: &Self) -> VWeak<T, A> {
        debug_assert!(!this.ptr.is_dangling());
        VWeak::new(this)
    }

    /// Demote without dynamically checking transaction boundaries
    pub unsafe fn unsafe_demote(&self) -> VWeak<T, A> {
        debug_assert!(!self.ptr.is_dangling());
        VWeak::new(self)
    }

    #[inline]
    /// Gets the number of `Weak` pointers to this allocation.
    ///
    /// # Examples
    ///
    /// ```
    /// # use corundum::alloc::*;
    /// # type P = Heap;
    /// use corundum::prc::Prc;
    ///
    /// P::transaction(|j| {
    ///     let five = Prc::new(5, j);
    ///
    ///     let _weak_five = Prc::downgrade(&five, j);
    ///     assert_eq!(1, Prc::weak_count(&five));
    /// }).unwrap()
    /// ```
    pub fn weak_count(this: &Self) -> usize {
        this.weak() - 1
    }

    #[inline]
    /// Gets the number of `Weak` pointers to this allocation.
    ///
    /// # Examples
    ///
    /// ```
    /// # use corundum::alloc::*;
    /// # type P = Heap;
    /// use corundum::prc::Prc;
    /// use corundum::clone::PClone;
    ///
    /// P::transaction(|j| {
    ///     let five = Prc::new(5, j);
    ///     let _also_five = Prc::pclone(&five, j);
    ///     assert_eq!(2, Prc::strong_count(&five));
    /// }).unwrap();
    /// ```
    pub fn strong_count(this: &Self) -> usize {
        this.strong()
    }

    #[inline]
    fn is_unique(this: &Self) -> bool {
        Prc::weak_count(this) == 0 && Prc::strong_count(this) == 1
    }

    #[inline]
    /// Returns `true` if the two `Prc`s point to the same allocation
    /// (in a vein similar to [`ptr::eq`]).
    ///
    /// # Examples
    ///
    /// ```
    /// # use corundum::alloc::*;
    /// # type P = Heap;
    /// use corundum::prc::Prc;
    /// use corundum::clone::PClone;
    ///
    /// P::transaction(|j| {
    ///     let five = Prc::new(5, j);
    ///     let same_five = Prc::pclone(&five, j);
    ///     let other_five = Prc::new(5, j);
    ///
    ///     assert!(Prc::ptr_eq(&five, &same_five));
    ///     assert!(!Prc::ptr_eq(&five, &other_five));
    /// }).unwrap();
    /// ```
    /// 
    /// [`ptr::eq`]: std::ptr::eq
    pub fn ptr_eq(this: &Self, other: &Self) -> bool {
        this.ptr.off() == other.ptr.off()
    }
}

impl<T: PSafe, A: MemPool> PmemUsage for Prc<T, A> {
    default fn size_of() -> usize {
        Ptr::<PrcBox<T, A>, A>::size_of()
    }
}

impl<T: PSafe + PmemUsage + ?Sized, A: MemPool> PmemUsage for Prc<T, A> {
    fn size_of() -> usize {
        Ptr::<PrcBox<T, A>, A>::size_of() + T::size_of()
    }
}

impl<T: PSafe + ?Sized, A: MemPool> Deref for Prc<T, A> {
    type Target = T;

    #[inline(always)]
    fn deref(&self) -> &T {
        &self.inner().value
    }
}


impl<T: PSafe, A: MemPool> Prc<T, A> {
    /// Initializes boxed data with `value` in-place if it is `None`
    ///
    /// This function should not be called from a transaction as it updates
    /// data without taking high-level logs. If transaction is unsuccessful,
    /// there is no way to recover data.
    /// However, it is safe to use it outside a transaction because it uses
    /// low-level logs to provide safety for a single update without drop.
    /// A dynamic check at the beginning makes sure of that.
    ///
    /// # Examples
    ///
    /// ```
    /// use corundum::default::*;
    /// 
    /// type P = BuddyAlloc;
    ///
    /// let root = P::open::<Option<Prc<i32>>>("foo.pool", O_CF).unwrap();
    ///
    /// Prc::initialize(&*root, 25);
    /// 
    /// let value = **root.as_ref().unwrap();
    /// assert_eq!(value, 25);
    /// ```
    ///
    pub fn initialize(rc: &Option<Prc<T, A>>, value: T) -> crate::result::Result<()> {
        assert!(
            !Journal::<A>::is_running(),
            "Prc::initialize() cannot be used inside a transaction"
        );
        match rc {
            Some(_) => Err("already initialized".to_string()),
            None => if A::valid(rc) {
                unsafe {
                    let new = A::atomic_new(
                        PrcBox::<T, A> {
                            counter: Counter {
                                strong: 1,
                                weak: 1,
        
                                #[cfg(not(feature = "no_log_rc"))]
                                has_log: 0,
                            },
        
                            #[cfg(not(feature = "no_volatile_pointers"))]
                            vlist: VCell::new(VWeakList::default()),
        
                            dummy: [],
                            value,
                        });
                    let pnew = Some(Prc::<T, A>::from_inner(Ptr::from_off_unchecked(new.1)));
                    let src = crate::utils::as_slice64(&pnew);
                    let mut base = A::off_unchecked(rc);
                    for i in src {
                        A::log64(base, *i, new.3);
                        base += 8;
                    }
                    persist_obj(rc);
                    A::perform(new.3);
                }
                Ok(())
            } else {
                Err("The object is not in the PM".to_string())
            }
        }
    }
}

unsafe impl<#[may_dangle] T: PSafe + ?Sized, A: MemPool> Drop for Prc<T, A> {
    /// Drops the `Prc` safely
    ///
    /// This will decrement the strong reference count. If the strong reference
    /// count reaches zero then the only other references (if any) are
    /// `Weak`, so we `drop` the inner value on commit using a `DropOnCommit` log.
    ///
    /// # Examples
    ///
    /// ```
    /// # use corundum::alloc::*;
    /// # type P = Heap;
    /// use corundum::prc::Prc;
    /// use corundum::clone::PClone;
    ///
    /// struct Foo;
    ///
    /// impl Drop for Foo {
    ///     fn drop(&mut self) {
    ///         println!("dropped!");
    ///     }
    /// }
    ///
    /// P::transaction(|j| {
    ///     let foo  = Prc::new(Foo, j);
    ///     let foo2 = Prc::pclone(&foo, j);
    ///
    ///     drop(foo);    // Doesn't print anything
    ///     drop(foo2);   // Prints "dropped!"
    /// }).unwrap();
    /// ```
    ///
    fn drop(&mut self) {
        unsafe {
            let journal = Journal::<A>::current(true).unwrap();
            self.dec_strong(journal.0);
            if self.strong() == 0 { // TODO: Add "or it is unreachable from the root"
                // destroy the contained object
                std::ptr::drop_in_place(&mut self.ptr.as_mut().value);

                self.dec_weak(journal.0);
                if self.weak() == 0 {
                    A::free(self.ptr.as_mut());

                    #[cfg(not(feature = "no_volatile_pointers"))]
                    std::ptr::drop_in_place(&mut self.ptr.as_mut().vlist);
                }
            }
        }
    }
}

impl<T: PSafe + ?Sized, A: MemPool> PClone<A> for Prc<T, A> {
    #[inline]
    /// Creates a new strong reference to the object
    /// 
    /// It increments the strong reference counter in a failure-atomic manner.
    /// When a transaction is aborted or power fails, every strong references
    /// to the object should be gone, and the counters should rollback to the
    /// consistent state before the transaction.
    /// 
    /// # Examples
    /// 
    /// ```
    /// # use corundum::default::*;
    /// # type P = BuddyAlloc;
    /// let root = P::open::<Prc<i32>>("foo.pool", O_CF).unwrap();
    /// 
    /// let _ = P::transaction(|j| {
    ///     let _n1 = root.pclone(j);
    ///     let _n2 = root.pclone(j);
    ///     let _n3 = root.pclone(j);
    ///     assert_eq!(4, Prc::strong_count(&root));
    ///     panic!("abort")
    /// });
    /// 
    /// assert_eq!(1, Prc::strong_count(&root));
    /// ```
    fn pclone(&self, journal: &Journal<A>) -> Prc<T, A> {
        self.inc_strong(journal);
        Self::from_inner(self.ptr)
    }
}

impl<T: RootObj<A> + PSafe, A: MemPool> RootObj<A> for Prc<T, A> {
    #[inline]
    default fn init(journal: &Journal<A>) -> Prc<T, A> {
        Prc::new(T::init(journal), journal)
    }
}

impl<T: Default + PSafe + ?Sized, A: MemPool> RootObj<A> for Prc<T, A> {
    #[inline]
    default fn init(journal: &Journal<A>) -> Prc<T, A> {
        Prc::new(T::default(), journal)
    }
}

trait RcEqIdent<T: PartialEq + PSafe + ?Sized, A: MemPool> {
    fn eq(&self, other: &Prc<T, A>) -> bool;
    fn ne(&self, other: &Prc<T, A>) -> bool;
}

impl<T: PartialEq + PSafe + ?Sized, A: MemPool> RcEqIdent<T, A> for Prc<T, A> {
    #[inline]
    fn eq(&self, other: &Prc<T, A>) -> bool {
        **self == **other
    }

    #[inline]
    fn ne(&self, other: &Prc<T, A>) -> bool {
        **self != **other
    }
}

impl<T: PartialEq + PSafe + ?Sized, A: MemPool> PartialEq for Prc<T, A> {
    #[inline]
    fn eq(&self, other: &Prc<T, A>) -> bool {
        RcEqIdent::eq(self, other)
    }
}

impl<T: Eq + PSafe + ?Sized, A: MemPool> Eq for Prc<T, A> {}

impl<T: PartialOrd + PSafe + ?Sized, A: MemPool> PartialOrd for Prc<T, A> {
    #[inline(always)]
    fn partial_cmp(&self, other: &Prc<T, A>) -> Option<Ordering> {
        (**self).partial_cmp(&**other)
    }

    #[inline(always)]
    fn lt(&self, other: &Prc<T, A>) -> bool {
        **self < **other
    }

    #[inline(always)]
    fn le(&self, other: &Prc<T, A>) -> bool {
        **self <= **other
    }

    #[inline(always)]
    fn gt(&self, other: &Prc<T, A>) -> bool {
        **self > **other
    }

    #[inline(always)]
    fn ge(&self, other: &Prc<T, A>) -> bool {
        **self >= **other
    }
}

impl<T: Ord + PSafe + ?Sized, A: MemPool> Ord for Prc<T, A> {
    #[inline]
    fn cmp(&self, other: &Prc<T, A>) -> Ordering {
        (**self).cmp(&**other)
    }
}

impl<T: Hash + PSafe + ?Sized, A: MemPool> Hash for Prc<T, A> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        (**self).hash(state);
    }
}

impl<T: fmt::Display + PSafe + ?Sized, A: MemPool> fmt::Display for Prc<T, A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&**self, f)
    }
}

impl<T: fmt::Debug + PSafe + ?Sized, A: MemPool> fmt::Debug for Prc<T, A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.deref().fmt(f)
    }
}

impl<T: PSafe + ?Sized, A: MemPool> fmt::Pointer for Prc<T, A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Pointer::fmt(&(&**self as *const T), f)
    }
}

/// `Weak` is a version of [`Prc`] that holds a non-owning reference to the
/// managed allocation. The allocation is accessed by calling [`upgrade`] on the `Weak`
/// pointer, which returns an [`Option`]`<`[`Prc`]`<T>>`.
///
/// Since a `Weak` reference does not count towards ownership, it will not
/// prevent the value stored in the allocation from being dropped, and `Weak` itself makes no
/// guarantees about the value still being present. Thus it may return [`None`]
/// when [`upgrade`]d. Note however that a `Weak` reference *does* prevent the allocation
/// itself (the backing store) from being deallocated.
///
/// A `Weak` pointer is useful for keeping a temporary reference to the allocation
/// managed by [`Prc`] without preventing its inner value from being dropped. It is also used to
/// prevent circular references between [`Prc`] pointers, since mutual owning references
/// would never allow either [`Prc`] to be dropped. For example, a tree could
/// have strong [`Prc`] pointers from parent nodes to children, and `Weak`
/// pointers from children back to their parents.
///
/// The typical way to obtain a `Weak` pointer is to call [`Prc::downgrade`].
///
/// [`Prc`]: struct.Prc.html
/// [`Prc::downgrade`]: ./struct.Prc.html#method.downgrade
/// [`upgrade`]: #method.upgrade
/// [`Option`]: std::option::Option
/// [`None`]: std::option::Option::None
pub struct Weak<T: PSafe + ?Sized, A: MemPool> {
    ptr: Ptr<PrcBox<T, A>, A>,
}

impl<T: ?Sized, A: MemPool> !TxOutSafe for Weak<T, A> {}
impl<T: ?Sized, A: MemPool> !Send for Weak<T, A> {}
impl<T: ?Sized, A: MemPool> !Sync for Weak<T, A> {}
impl<T: ?Sized, A: MemPool> !VSafe for Weak<T, A> {}
impl<T: ?Sized, A: MemPool> !PSend for Weak<T, A> {}

impl<T: PSafe, A: MemPool> Weak<T, A> {
    pub fn as_raw(&self) -> *const T {
        match self.inner() {
            None => std::ptr::null(),
            Some(inner) => {
                let offset = data_offset_sized::<T, A>();
                let ptr = inner as *const PrcBox<T, A>;
                // Note: while the pointer we create may already point to dropped value, the
                // allocation still lives (it must hold the weak point as long as we are alive).
                // Therefore, the offset is OK to do, it won't get out of the allocation.
                let ptr = unsafe { (ptr as *const u8).offset(offset) };
                ptr as *const T
            }
        }
    }

    pub fn into_raw(self) -> *const T {
        let result = self.as_raw();
        mem::forget(self);
        result
    }

    #[allow(clippy::missing_safety_doc)]
    pub unsafe fn from_raw(ptr: *const T) -> Self {
        if ptr.is_null() {
            Self::new()
        } else {
            // See Rc::from_raw for details
            let offset = data_offset::<T, A>(ptr);
            let fake_ptr = ptr as *mut PrcBox<T, A>;
            let ptr = set_data_ptr(fake_ptr, (ptr as *mut u8).offset(-offset));
            Weak {
                ptr: Ptr::from_raw(ptr),
            }
        }
    }
}

impl<T: PSafe + ?Sized, A: MemPool> Weak<T, A> {
    /// Creates a new dangling weak pointer
    pub fn new() -> Weak<T, A> {
        Weak {
            ptr: Ptr::dangling(),
        }
    }

    pub fn upgrade(&self, journal: &Journal<A>) -> Option<Prc<T, A>> {
        let inner = self.inner()?;
        if inner.strong() == 0 {
            None
        } else {
            inner.inc_strong(journal);
            Some(Prc::from_inner(self.ptr))
        }
    }

    pub fn strong_count(&self) -> usize {
        if let Some(inner) = self.inner() {
            inner.strong()
        } else {
            0
        }
    }

    pub fn weak_count(&self) -> Option<usize> {
        self.inner().map(|inner| {
            if inner.strong() > 0 {
                inner.weak() - 1 // subtract the implicit weak ptr
            } else {
                inner.weak()
            }
        })
    }

    #[inline]
    fn inner(&self) -> Option<&PrcBox<T, A>> {
        if self.ptr.is_dangling() {
            None
        } else {
            Some(self.ptr.get_mut())
        }
    }

    #[inline]
    pub fn ptr_eq(&self, other: &Self) -> bool {
        self.ptr == other.ptr
    }
}

impl<T: PSafe + ?Sized, A: MemPool> Drop for Weak<T, A> {
    fn drop(&mut self) {
        if let Some(inner) = self.inner() {
            let journal = Journal::<A>::current(true).unwrap();
            inner.dec_weak(journal.0);
            if inner.weak() == 0 {
                unsafe {
                    A::free(self.ptr.as_mut());

                    #[cfg(not(feature = "no_volatile_pointers"))]
                    std::ptr::drop_in_place(&mut self.ptr.as_mut().vlist);
                }
            }
        }
    }
}

impl<T: PSafe + ?Sized, A: MemPool> PClone<A> for Weak<T, A> {
    #[inline]
    fn pclone(&self, journal: &Journal<A>) -> Weak<T, A> {
        if let Some(inner) = self.inner() {
            inner.inc_weak(journal)
        }
        Weak { ptr: self.ptr }
    }
}

impl<T: PSafe + fmt::Debug + ?Sized, A: MemPool> fmt::Debug for Weak<T, A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "(Weak)")
    }
}

impl<T: PSafe + ?Sized, A: MemPool> RootObj<A> for Weak<T, A> {
    fn init(_: &Journal<A>) -> Weak<T, A> {
        Weak::new()
    }
}

trait PrcBoxPtr<T: PSafe + ?Sized, A: MemPool> {
    #[allow(clippy::mut_from_ref)]
    fn count(&self) -> &mut Counter;

    #[inline]
    fn strong(&self) -> usize {
        self.count().strong
    }

    #[inline]
    #[cfg(not(feature = "no_log_rc"))]
    fn log_count(&self, journal: *const Journal<A>) {
        let inner = self.count();

        if inner.has_log == 0 {
            unsafe {
                inner.take_log(&*journal, Notifier::NonAtomic(Ptr::from_ref(&inner.has_log)));
            }
        }
    }

    #[inline]
    fn inc_strong(&self, _journal: *const Journal<A>) {
        let inner = self.count();
        let strong = inner.strong;

        if strong == 0 || strong == usize::max_value() {
            std::process::abort();
        }
        #[cfg(not(feature = "no_log_rc"))]
        self.log_count(_journal);

        inner.strong += 1;
    }

    #[inline]
    fn dec_strong(&self, _journal: *const Journal<A>) {
        #[cfg(not(feature = "no_log_rc"))]
        self.log_count(_journal);

        self.count().strong -= 1;
    }

    #[inline]
    fn weak(&self) -> usize {
        self.count().weak
    }

    #[inline]
    fn inc_weak(&self, _journal: *const Journal<A>) {
        let weak = self.weak();

        if weak == 0 || weak == usize::max_value() {
            std::process::abort();
        }

        #[cfg(not(feature = "no_log_rc"))]
        self.log_count(_journal);

        self.count().weak += 1;
    }

    #[inline]
    fn dec_weak(&self, _journal: *const Journal<A>) {
        #[cfg(not(feature = "no_log_rc"))]
        self.log_count(_journal);

        self.count().weak -= 1;
    }
}

impl<T: PSafe + ?Sized, A: MemPool> PrcBoxPtr<T, A> for Prc<T, A> {
    #[inline(always)]
    fn count(&self) -> &mut Counter {
        &mut self.ptr.get_mut().counter
    }
}

impl<T: PSafe + ?Sized, A: MemPool> PrcBoxPtr<T, A> for PrcBox<T, A> {
    #[inline(always)]
    fn count(&self) -> &mut Counter {
        unsafe {
            let ptr: *const Self = self;
            let ptr: *mut Self = ptr as *mut Self;
            let rcbox: &mut Self = &mut *ptr;
            &mut rcbox.counter
        }
    }
}

impl<T: PSafe + ?Sized, A: MemPool> borrow::Borrow<T> for Prc<T, A> {
    fn borrow(&self) -> &T {
        &self.inner().value
    }
}

impl<T: PSafe + ?Sized, A: MemPool> AsRef<T> for Prc<T, A> {
    fn as_ref(&self) -> &T {
        &self.inner().value
    }
}

impl<T: PSafe + ?Sized, A: MemPool> Unpin for Prc<T, A> {}

unsafe fn data_offset<T: ?Sized, A: MemPool>(ptr: *const T) -> isize {
    data_offset_align::<A>(mem::align_of_val(&*ptr))
}

fn data_offset_sized<T, A: MemPool>() -> isize {
    data_offset_align::<A>(mem::align_of::<T>())
}

#[inline]
fn data_offset_align<A: MemPool>(align: usize) -> isize {
    let layout = std::alloc::Layout::new::<PrcBox<(), A>>();
    (layout.size() + layout.padding_needed_for(align)) as isize
}

pub fn ws<T: PSafe, A: MemPool>(ptr: &Prc<T, A>) -> (usize, usize) {
    let i = ptr.inner();
    (i.strong(), i.weak())
}

/// `VWeak` is a version of [`Prc`] that holds a non-owning reference to the
/// managed allocation in the demote heap. The allocation is accessed by
/// calling [`promote`] on the `VWeak` pointer, which returns an
/// [`Option`]`<`[`Prc`]`<T>>`.
///
/// Since a `VWeak` reference does not count towards ownership, it will not
/// prevent the value stored in the allocation from being dropped, and `VWeak`
/// itself makes no guarantees about the value still being present. Thus it may
/// return [`None`] when [`promote`]d. Note however that a `VWeak` reference,
/// unlike [`Weak`], *does NOT* prevent the allocation itself (the backing
/// store) from being deallocated.
///
/// A `VWeak` pointer is useful for keeping a temporary reference to the
/// persistent allocation managed by [`Prc`] without preventing its inner value
/// from being dropped from ... It is also used to
/// prevent circular references between [`Prc`] pointers, since mutual owning references
/// would never allow either [`Prc`] to be dropped. For example, a tree could
/// have strong [`Prc`] pointers from parent nodes to children, and `Weak`
/// pointers from children back to their parents.
///
/// The typical way to obtain a `VWeak` pointer is to call [`Prc::demote`].
///
/// [`Prc`]: struct.Prc.html
/// [`Weak`]: struct.Weak.html
/// [`Prc::downgrade`]: ./struct.Prc.html#method.downgrade
/// [`promote`]: #method.promote
/// [`Option`]: std::option::Option
/// [`None`]: std::option::Option::None
pub struct VWeak<T: ?Sized, A: MemPool> {
    ptr: *const PrcBox<T, A>,
    valid: *mut VWeakValid,
    gen: u32,
}

impl<T: ?Sized, A: MemPool> !Send for VWeak<T, A> {}
impl<T: ?Sized, A: MemPool> !Sync for VWeak<T, A> {}
impl<T: ?Sized, A: MemPool> !PSend for VWeak<T, A> {}
impl<T: ?Sized, A: MemPool> UnwindSafe for VWeak<T, A> {}
impl<T: ?Sized, A: MemPool> RefUnwindSafe for VWeak<T, A> {}
unsafe impl<T: ?Sized, A: MemPool> TxInSafe for VWeak<T, A> {}
unsafe impl<T: ?Sized, A: MemPool> TxOutSafe for VWeak<T, A> {}
unsafe impl<T: ?Sized, A: MemPool> PSafe for VWeak<T, A> {}

impl<T: PSafe + ?Sized, A: MemPool> VWeak<T, A> {
    fn new(prc: &Prc<T, A>) -> VWeak<T, A> {
        let list = prc.ptr.vlist.as_mut();
        VWeak {
            ptr: prc.ptr.as_ref(),
            valid: list.append(),
            gen: A::gen(),
        }
    }

    pub fn null() -> VWeak<T, A> where T: Sized {
        VWeak {
            ptr: std::ptr::null(),
            valid: std::ptr::null_mut(),
            gen: u32::MAX,
        }
    }

    pub fn promote(&self, journal: &Journal<A>) -> Option<Prc<T, A>> {
        let inner = self.inner()?;
        let strong = inner.counter.strong;
        if strong == 0 {
            None
        } else {
            unsafe { Some(Prc::from_ptr(self.ptr as *const _ as *mut _, journal)) }
        }
    }

    #[inline]
    fn inner(&self) -> Option<&PrcBox<T, A>> {
        unsafe {
            if self.gen != A::gen() {
                None
            } else if !(*self.valid).valid {
                None
            } else {
                Some(&*self.ptr)
            }
        }
    }
}

impl<T: PSafe + ?Sized, A: MemPool> Clone for VWeak<T, A> {
    fn clone(&self) -> Self {
        if self.gen == A::gen() {
            unsafe { 
                if (*self.valid).valid {
                    let list = (*self.ptr).vlist.as_mut();
                    return VWeak {
                        ptr: self.ptr,
                        valid: list.append(),
                        gen: self.gen,
                    };  
                }
            }
        } 
        VWeak {
            ptr: self.ptr,
            valid: self.valid,
            gen: self.gen,
        }
    }
}


impl<T: ?Sized, A: MemPool> Drop for VWeak<T, A> {
    fn drop(&mut self) {
        unsafe {
            let this = &mut *self.valid;
            if A::is_open() {
                if self.gen == A::gen() {
                    if !this.list.is_null() {
                        let head = &mut (*this.list).head;
                        if this.prev.is_null() {
                            *head = this.next;
                        } else {
                            (*this.prev).next = this.next;
                        }
                        if !this.next.is_null() {
                            (*this.next).prev = this.prev;
                        }
                    }
                }
            }
        }
    }
}

struct VWeakValid {
    valid: bool,
    next: *mut VWeakValid,
    prev: *mut VWeakValid,
    list: *mut VWeakList,
}

struct VWeakList {
    head: *mut VWeakValid,
}

impl VWeakList {
    fn append(&mut self) -> *mut VWeakValid {
        let new = Box::into_raw(Box::new(VWeakValid {
            valid: true,
            next: self.head,
            prev: std::ptr::null_mut(),
            list: self as *mut Self,
        }));
        if !self.head.is_null() {
            unsafe {
                (*self.head).prev = new;
            }
        }
        self.head = new;
        new
    }
}

impl Default for VWeakList {
    fn default() -> Self {
        VWeakList {
            head: std::ptr::null_mut(),
        }
    }
}

impl Drop for VWeakList {
    fn drop(&mut self) {
        unsafe {
            let mut curr = self.head;
            while !curr.is_null() {
                (*curr).valid = false;
                (*curr).list = std::ptr::null_mut();
                curr = (*curr).next;
            }
        }
    }
}
