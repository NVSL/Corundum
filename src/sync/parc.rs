use std::panic::RefUnwindSafe;
use std::panic::UnwindSafe;
use crate::alloc::{MemPool, PmemUsage};
use crate::cell::VCell;
use crate::clone::*;
use crate::ptr::Ptr;
use crate::stm::*;
use crate::*;
use std::clone::Clone as StdClone;
use std::cmp::Ordering;
use std::hash::Hash;
use std::hash::Hasher;
use std::marker::PhantomData;
use std::mem::MaybeUninit;
use std::ops::Deref;
use std::sync::atomic::{self, AtomicBool, AtomicUsize, Ordering::*};
use std::*;

const MAX_REFCOUNT: usize = (isize::MAX) as usize;

#[derive(Debug)]
struct Counter {
    strong: AtomicUsize,
    weak: AtomicUsize,

    #[cfg(not(feature = "no_log_rc"))]
    has_log: u8,
}

unsafe impl PSafe for Counter {}

/// The [`Parc`]'s inner data type
/// 
/// It contains the atomic counters, a list of demote references, and the
/// actual value.
/// 
/// [`Parc`]: #
pub struct ParcInner<T: ?Sized, A: MemPool> {
    counter: Counter,

    #[cfg(not(feature = "no_volatile_pointers"))]
    vlist: VCell<VWeakList, A>,

    marker: PhantomData<A>,
    value: T,
}

unsafe impl<T: PSafe + ?Sized, A: MemPool> PSafe for ParcInner<T, A> {}
impl<T: ?Sized, A: MemPool> !VSafe for ParcInner<T, A> {}

unsafe fn set_data_ptr<T, U>(mut ptr: *mut T, data: *mut U) -> *mut T {
    std::ptr::write(&mut ptr as *mut _ as *mut *mut u8, data as *mut u8);
    ptr
}

/// A thread-safe reference-counting persistent pointer. 'Parc' stands for
/// 'Persistent Atomically Reference Counted'.
///
/// The main aspect of `Parc<T>` is that its atomic counters are also 
/// transactional to provide failure atomicity which means that functions
/// [`pclone`], [`downgrade`], and [`upgrade`] require a [`Journal`] to operate. 
/// In other words, you need to wrap them in a [`transaction`]. The counters are
/// atomic, so it is safe to share it in multiple threads.
/// 
/// Unlike [`Arc`], `Parc` does not implement [`Send`] to prevent memory leak. 
/// The reason is that if a `Parc` is created in a transaction without being
/// reachable from the root object, and moves to a thread, due to being RAII,
/// its drop function gets called in the other thread outside the original
/// transaction. Therefore, it destroys allocation consistency and leaves the
/// `Parc` unreachable in the memory if a crash happens between the original
/// transaction is done and the drop function is called.
/// 
/// To allow sharing, `Parc` provides a safe mechanism to cross the thread
/// boundaries. When you need to share it, you can obtain a [`VWeak`]
/// object by calling [`demote()`] function. The [`VWeak`] object is both
/// [`Sync`] and [`Send`] and acts like a demote reference. Calling
/// [`VWeak`]`::`[`promote()`] gives access to data by creating a new reference
/// of type `Parc` inside the other thread, if the referent is still available.
/// Calling [`demote()`] is dynamically prohibited to be inside a transaction.
/// Therefore, the `Parc` should be already reachable from the root object and
/// packed outside a transaction.
///
/// # Examples
///
/// ```
/// use corundum::default::*;
/// use std::thread;
/// 
/// type P = BuddyAlloc;
/// 
/// let p = P::open::<Parc<i32>>("foo.pool", O_CF).unwrap();
/// let v = p.demote();
/// let mut threads = vec![];
/// 
/// for i in 0..10 {
///     let p = v.clone();
///     threads.push(thread::spawn(move || {
///         transaction(|j| {
///             if let Some(p) = p.promote(j) {
///                 println!("access {} from thread {}", *p, i);
///             }
///         }).unwrap();
///     }));
/// }
/// 
/// for t in threads {
///     t.join().unwrap();
/// }
/// ```
///
/// # Mutability
///
/// `Parc` doesn't provide mutable reference to the inner value. To allow
/// interior mutability, you may use `Parc<`[`Mutex`]`<T,P>,P>` (or in short, 
/// `Parc<`[`PMutex`]`<T>>` using aliased types).
///
/// ```
/// use corundum::default::*;
/// use std::thread;
/// 
/// type P = BuddyAlloc;
/// 
/// let p = P::open::<Parc<PMutex<i32>>>("foo.pool", O_CF).unwrap();
/// let v = p.demote();
/// let mut threads = vec![];
/// 
/// for i in 0..10 {
///     let p = v.clone();
///     threads.push(thread::spawn(move || {
///         transaction(|j| {
///             if let Some(p) = p.promote(j) {
///                 let mut p = p.lock(j);
///                 *p += 1;
///                 println!("thread {} makes it {}", i, *p);
///             }
///         }).unwrap();
///     }));
/// }
/// 
/// for t in threads {
///     t.join().unwrap();
/// }
/// 
/// let res = transaction(|j| {
///     *p.lock(j)
/// }).unwrap();
/// 
/// assert_eq!(res, 10);
/// ```
///
/// [`downgrade`]: #method.downgrade
/// [`upgrade`]: ./struct.Weak.html#method.upgrade
/// [`Journal`]: ../stm/journal/struct.Journal.html
/// [`transaction`]: ../stm/fn.transaction.html
/// [`Arc`]: std::sync::Arc
/// [`Mutex`]: ./struct.Mutex.html
/// [`PMutex`]: ../alloc/default/type.PMutex.html
/// [`pclone`]: #impl-PClone
/// [`demote()`]: #method.demote
/// [`promote()`]: ./struct.VWeak.html#method.promote
pub struct Parc<T: PSafe + ?Sized, A: MemPool> {
    ptr: Ptr<ParcInner<T, A>, A>,
    phantom: PhantomData<T>,
}

impl<T: ?Sized, A: MemPool> !TxOutSafe for Parc<T, A> {}
impl<T: ?Sized, A: MemPool> !Send for Parc<T, A> {}
impl<T: ?Sized, A: MemPool> !VSafe for Parc<T, A> {}

impl<T: PSafe + ?Sized, A: MemPool> UnwindSafe for Parc<T, A> {}
impl<T: PSafe + ?Sized, A: MemPool> RefUnwindSafe for Parc<T, A> {}
unsafe impl<T: PSafe + ?Sized, A: MemPool> TxInSafe for Parc<T, A> {}

impl<T: PSafe, A: MemPool> Parc<T, A> {
    /// Constructs a new `Parc<T>`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use corundum::alloc::*;
    /// use corundum::sync::Parc;
    ///
    /// Heap::transaction(|j| {
    ///     let five = Parc::new(5, j);
    /// }).unwrap();
    /// ```
    pub fn new(value: T, journal: &Journal<A>) -> Parc<T, A> {
        unsafe {
            let ptr = Ptr::new_unchecked(A::new(
                ParcInner::<T, A> {
                    counter: Counter {
                        strong: AtomicUsize::new(1),
                        weak: AtomicUsize::new(1),

                        #[cfg(not(feature = "no_log_rc"))]
                        has_log: 0,
                    },

                    #[cfg(not(feature = "no_volatile_pointers"))]
                    vlist: VCell::new(VWeakList::default()),

                    marker: PhantomData,
                    value,
                },
                journal,
            ));
            Self::from_inner(ptr)
        }
    }

    /// Constructs a new `Parc` with uninitialized contents.
    ///
    /// # Examples
    ///
    /// ```
    /// use corundum::alloc::*;
    /// use corundum::sync::Parc;
    ///
    /// corundum::transaction(|j| {
    ///     let mut five = Parc::<u32,Heap>::new_uninit(j);
    ///
    ///     let five = unsafe {
    ///         // Deferred initialization:
    ///         Parc::get_mut_unchecked(&mut five).as_mut_ptr().write(5);
    ///
    ///         five.assume_init()
    ///     };
    ///
    ///     assert_eq!(*five, 5)
    /// }).unwrap();
    /// ```
    pub fn new_uninit(journal: &Journal<A>) -> Parc<MaybeUninit<T>, A> {
        unsafe {
            Parc::from_inner(Ptr::from_mut(A::new(
                ParcInner {
                    counter: Counter {
                        strong: AtomicUsize::new(1),
                        weak: AtomicUsize::new(1),

                        #[cfg(not(feature = "no_log_rc"))]
                        has_log: 0,
                    },

                    #[cfg(not(feature = "no_volatile_pointers"))]
                    vlist: VCell::new(VWeakList::default()),

                    marker: PhantomData,
                    value: MaybeUninit::<T>::uninit(),
                },
                journal,
            )))
        }
    }

    /// Constructs a new `Parc` with uninitialized contents, with the memory
    /// being filled with `0` bytes.
    ///
    /// See `MaybeUninit::zeroed` for examples of correct and incorrect usage of
    /// this method.
    ///
    /// # Examples
    ///
    /// ```
    /// use corundum::alloc::*;
    /// use corundum::sync::Parc;
    ///
    /// Heap::transaction(|j| {
    ///     let zero = Parc::<u32,Heap>::new_zeroed(j);
    ///     let zero = unsafe { zero.assume_init() };
    ///
    ///     assert_eq!(*zero, 0)
    /// }).unwrap();
    /// ```
    ///
    pub fn new_zeroed(journal: &Journal<A>) -> Parc<mem::MaybeUninit<T>, A> {
        unsafe {
            let mut uninit = Self::new_uninit(journal);
            std::ptr::write_bytes::<T>(Parc::get_mut_unchecked(&mut uninit).as_mut_ptr(), 0, 1);
            uninit
        }
    }
}

impl<T: PSafe + ?Sized, A: MemPool> Parc<T, A> {
    #[inline]
    fn from_inner(ptr: Ptr<ParcInner<T, A>, A>) -> Self {
        Parc {
            ptr,
            phantom: PhantomData,
        }
    }

    #[inline(always)]
    fn inner(&self) -> &ParcInner<T, A> {
        self.ptr.as_ref()
    }

    #[allow(clippy::missing_safety_doc)]
    unsafe fn from_ptr(ptr: *mut ParcInner<T, A>, j: &Journal<A>) -> Self {
        let off = A::off_unchecked(ptr);
        let res = Self::from_inner(Ptr::from_off_unchecked(off));

        #[cfg(not(feature = "no_log_rc"))]
        res.log_count(j);

        res.inner().counter.strong.fetch_add(1, Relaxed);

        res
    }

    #[inline(never)]
    unsafe fn drop_slow(&mut self) {
        // Destroy the data at this time, even though we may not free the box
        // allocation itself (there may still be weak pointers lying around).
        std::ptr::drop_in_place(&mut self.ptr.as_mut().value);

        if self.inner().counter.weak.fetch_sub(1, Release) == 1 {
            atomic::fence(Acquire);
            A::free(self.ptr.as_mut());

            #[cfg(not(feature = "no_volatile_pointers"))]
            std::ptr::drop_in_place(self.ptr.as_mut().vlist.as_mut());
        }
    }
}

impl<T: PSafe, A: MemPool> Parc<mem::MaybeUninit<T>, A> {
    /// Converts to `Parc<T>`.
    ///
    /// # Safety
    ///
    /// As with [`MaybeUninit::assume_init`],
    /// it is up to the caller to guarantee that the inner value
    /// really is in an initialized state.
    /// Calling this when the content is not yet fully initialized
    /// causes immediate undefined behavior.
    ///
    /// [`MaybeUninit::assume_init`]: std::mem::MaybeUninit#method.assume_init
    ///
    /// # Examples
    ///
    /// ```
    /// use corundum::alloc::*;
    /// use corundum::sync::Parc;
    ///
    /// corundum::transaction(|j| {
    ///     let mut five = Parc::<u32,Heap>::new_uninit(j);
    ///
    ///     let five = unsafe {
    ///         // Deferred initialization:
    ///         Parc::get_mut_unchecked(&mut five).as_mut_ptr().write(5);
    ///
    ///         five.assume_init()
    ///     };
    ///
    ///     assert_eq!(*five, 5);
    /// }).unwrap();
    /// ```
    #[inline]
    pub unsafe fn assume_init(self) -> Parc<T, A> {
        Parc::from_inner(mem::ManuallyDrop::new(self).ptr.cast())
    }
}

impl<T: PSafe, A: MemPool> Parc<MaybeUninit<T>, A> {
    #[inline]

    /// Returns a mutable reference into the given `Parc`, if there are
    /// no other [`Parc`] or [`Weak`] pointers to the same allocation.
    ///
    /// Returns `None` otherwise, because it is not safe to mutate a shared
    /// value. It only works for `Parc<MaybeUninit<T>>` to be able to defer the
    /// initialization.
    ///
    /// # Examples
    ///
    /// ```
    /// use corundum::alloc::*;
    /// use corundum::sync::Parc;
    ///
    /// corundum::transaction(|j| {
    ///     let mut five = Parc::<u32,Heap>::new_uninit(j);
    ///
    ///     let five = unsafe {
    ///         // Deferred initialization:
    ///         Parc::get_mut(&mut five).unwrap().as_mut_ptr().write(5);
    ///
    ///         five.assume_init()
    ///     };
    ///
    ///     assert_eq!(*five, 5)
    /// }).unwrap();
    /// ```
    pub fn get_mut(this: &mut Self) -> Option<&mut MaybeUninit<T>> {
        if Parc::is_unique(this) {
            unsafe { Some(Parc::get_mut_unchecked(this)) }
        } else {
            None
        }
    }

    #[inline]
    /// Returns a mutable reference into the given `Parc`, without any check.
    ///
    /// It only works for `Parc<MaybeUninit<T>>` to be able to defer the
    /// initialization.
    ///
    /// # Safety
    ///
    /// Any other [`Parc`] or [`Weak`] pointers to the same allocation must not 
    /// be dereferenced for the duration of the returned borrow. This is 
    /// trivially the case if no such pointers exist, for example immediately
    /// after [`Parc::new`].
    ///
    /// # Examples
    ///
    /// ```
    /// use corundum::alloc::*;
    /// use corundum::sync::Parc;
    ///
    /// corundum::transaction(|j| {
    ///     let mut five = Parc::<u32,Heap>::new_uninit(j);
    ///
    ///     let five = unsafe {
    ///         // Deferred initialization:
    ///         Parc::get_mut_unchecked(&mut five).as_mut_ptr().write(5);
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

impl<T: PSafe + ?Sized, A: MemPool> Parc<T, A> {
    /// Creates a new [`Weak`] pointer to this allocation.
    /// 
    /// The [`Weak`] pointer can be [`upgrade`]d later in a transaction.
    ///
    /// # Examples
    ///
    /// ```
    /// use corundum::alloc::*;
    /// use corundum::sync::Parc;
    ///
    /// Heap::transaction(|j| {
    ///     let five = Parc::new(5, j);
    ///     let _weak_five = Parc::downgrade(&five, j);
    /// }).unwrap()
    /// ```
    /// 
    /// [`upgrade`]: ./struct.Weak.html#method.upgrade
    pub fn downgrade(this: &Self, _journal: &Journal<A>) -> Weak<T, A> {
        // This Relaxed is OK because we're checking the value in the CAS
        // below.
        let mut cur = this.inner().counter.weak.load(Relaxed);

        loop {
            // check if the weak counter is currently "locked"; if so, spin.
            if cur == usize::MAX {
                cur = this.inner().counter.weak.load(Relaxed);
                continue;
            }

            #[cfg(not(feature = "no_log_rc"))]
            this.log_count(_journal);

            // NOTE: this code currently ignores the possibility of overflow
            // into usize::MAX; in general both Rc and Arc need to be adjusted
            // to deal with overflow.

            // Unlike with Clone(), we need this to be an Acquire read to
            // synchronize with the write coming from `is_unique`, so that the
            // events prior to that write happen before this read.
            match this
                .inner()
                .counter
                .weak
                .compare_exchange_weak(cur, cur + 1, Acquire, Relaxed)
            {
                Ok(_) => {
                    // Make sure we do not create a dangling Weak
                    debug_assert!(!this.ptr.is_dangling());
                    return Weak {
                        ptr: this.ptr.clone(),
                    };
                }
                Err(old) => cur = old,
            }
        }
    }

    /// Creates a new sharable [`VWeak`](./struct.VWeak.html) pointer to this
    /// allocation.
    /// 
    /// # Errors
    /// 
    /// This function requires the allocation to be reachable from the
    /// persistent root. Therefore, it panics if it gets called inside a
    /// transaction.
    ///
    /// # Examples
    ///
    /// ```
    /// use corundum::default::*;
    /// 
    /// type P = BuddyAlloc;
    /// 
    /// let obj = P::open::<Parc<i32>>("foo.pool", O_CF).unwrap();
    /// 
    /// let v = obj.demote();
    /// assert_eq!(Parc::strong_count(&obj), 1);
    /// 
    /// P::transaction(|j| {
    ///     if let Some(obj) = v.promote(j) {
    ///         assert_eq!(Parc::strong_count(&obj), 2);
    ///     }
    /// }).unwrap();
    /// 
    /// assert_eq!(Parc::strong_count(&obj), 1);
    /// ```
    pub fn demote(&self) -> VWeak<T, A> {
        debug_assert!(!self.ptr.is_dangling());
        assert!(
            !Journal::<A>::is_running(),
            "Parc::demote() cannot be called from a transaction"
        );
        VWeak::new(self)
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
    /// use corundum::alloc::*;
    /// use corundum::sync::Parc;
    ///
    /// Heap::transaction(|j| {
    ///     let five = Parc::new(5, j);
    ///
    ///     let _weak_five = Parc::downgrade(&five, j);
    ///     assert_eq!(1, Parc::weak_count(&five));
    /// }).unwrap()
    /// ```
    pub fn weak_count(this: &Self) -> usize {
        let cnt = this.inner().counter.weak.load(SeqCst);
        // If the weak count is currently locked, the value of the
        // count was 0 just before taking the lock.
        if cnt == usize::MAX {
            0
        } else {
            cnt - 1
        }
    }

    #[inline]
    /// Gets the number of `Strong` pointers to this allocation.
    ///
    /// # Examples
    ///
    /// ```
    /// use corundum::alloc::*;
    /// use corundum::sync::Parc;
    /// use corundum::clone::PClone;
    ///
    /// Heap::transaction(|j| {
    ///     let five = Parc::new(5, j);
    ///     let _also_five = Parc::pclone(&five, j);
    ///     assert_eq!(2, Parc::strong_count(&five));
    /// }).unwrap();
    /// ```
    pub fn strong_count(this: &Self) -> usize {
        this.inner().counter.strong.load(SeqCst)
    }

    #[inline]
    fn is_unique(this: &Self) -> bool {
        Parc::weak_count(this) == 0 && Parc::strong_count(this) == 1
    }

    #[inline]
    /// Returns `true` if the two `Parc`s point to the same allocation
    /// (in a vein similar to [`std::ptr::eq`]).
    ///
    /// # Examples
    ///
    /// ```
    /// use corundum::alloc::*;
    /// use corundum::sync::Parc;
    /// use corundum::clone::PClone;
    ///
    /// Heap::transaction(|j| {
    ///     let five = Parc::new(5, j);
    ///     let same_five = Parc::pclone(&five, j);
    ///     let other_five = Parc::new(5, j);
    ///
    ///     assert!(Parc::ptr_eq(&five, &same_five));
    ///     assert!(!Parc::ptr_eq(&five, &other_five));
    /// }).unwrap();
    /// ```
    pub fn ptr_eq(this: &Self, other: &Self) -> bool {
        this.ptr.off() == other.ptr.off()
    }
}

impl<T: PSafe, A: MemPool> PmemUsage for Parc<T, A> {
    default fn size_of() -> usize {
        Ptr::<ParcInner<T, A>, A>::size_of()
    }
}

impl<T: PSafe + PmemUsage + ?Sized, A: MemPool> PmemUsage for Parc<T, A> {
    fn size_of() -> usize {
        Ptr::<ParcInner<T, A>, A>::size_of() + T::size_of()
    }
}

impl<T: PSafe + ?Sized, A: MemPool> Deref for Parc<T, A> {
    type Target = T;

    #[inline(always)]
    fn deref(&self) -> &T {
        &self.inner().value
    }
}

unsafe impl<#[may_dangle] T: PSafe + ?Sized, A: MemPool> Drop for Parc<T, A> {
    /// Drops the `Parc` safely
    ///
    /// This will decrement the strong reference count. If the strong reference
    /// count reaches zero then the only other references (if any) are
    /// `Weak`, so we `drop` the inner value on commit using a `DropOnCommit` log.
    ///
    /// # Examples
    ///
    /// ```
    /// use corundum::alloc::*;
    /// use corundum::sync::Parc;
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
    /// Heap::transaction(|j| {
    ///     let foo  = Parc::new(Foo, j);
    ///     let foo2 = Parc::pclone(&foo, j);
    ///
    ///     drop(foo);    // Doesn't print anything
    ///     drop(foo2);   // Prints "dropped!"
    /// }).unwrap();
    /// ```
    ///
    fn drop(&mut self) {
        unsafe {
            #[cfg(not(feature = "no_log_rc"))]
            {
                let journal = Journal::<A>::current(true).unwrap();
                self.log_count(journal.0);
            }

            // Because `fetch_sub` is already atomic, we do not need to synchronize
            // with other threads unless we are going to delete the object. This
            // same logic applies to the below `fetch_sub` to the `weak` count.
            if self.inner().counter.strong.fetch_sub(1, Release) != 1 {
                return;
            }

            atomic::fence(Acquire);

            self.drop_slow();
        }
    }
}

impl<T: PSafe + ?Sized, A: MemPool> PClone<A> for Parc<T, A> {
    #[inline]
    fn pclone(&self, _journal: &Journal<A>) -> Parc<T, A> {
        #[cfg(not(feature = "no_log_rc"))]
        self.log_count(_journal);

        let old_size = self.inner().counter.strong.fetch_add(1, Relaxed);

        // However we need to guard against massive ref counts in case someone
        // is `mem::forget`ing Arcs. If we don't do this the count can overflow
        // and users will use-after free. We racily saturate to `isize::MAX` on
        // the assumption that there aren't ~2 billion threads incrementing
        // the reference count at once. This branch will never be taken in
        // any realistic program.
        //
        // We abort because such a program is incredibly degenerate, and we
        // don't care to support it.
        if old_size > MAX_REFCOUNT {
            std::process::abort();
        }

        Self::from_inner(self.ptr)
    }
}

impl<T: RootObj<A> + PSafe, A: MemPool> RootObj<A> for Parc<T, A> {
    #[inline]
    default fn init(journal: &Journal<A>) -> Parc<T, A> {
        Parc::new(T::init(journal), journal)
    }
}

impl<T: Default + PSafe + ?Sized, A: MemPool> RootObj<A> for Parc<T, A> {
    #[inline]
    default fn init(journal: &Journal<A>) -> Parc<T, A> {
        Parc::new(T::default(), journal)
    }
}

trait RcEqIdent<T: PartialEq + PSafe + ?Sized, A: MemPool> {
    fn eq(&self, other: &Parc<T, A>) -> bool;
    fn ne(&self, other: &Parc<T, A>) -> bool;
}

impl<T: PartialEq + PSafe + ?Sized, A: MemPool> RcEqIdent<T, A> for Parc<T, A> {
    #[inline]
    fn eq(&self, other: &Parc<T, A>) -> bool {
        **self == **other
    }

    #[inline]
    fn ne(&self, other: &Parc<T, A>) -> bool {
        **self != **other
    }
}

impl<T: PartialEq + PSafe + ?Sized, A: MemPool> PartialEq for Parc<T, A> {
    #[inline]
    fn eq(&self, other: &Parc<T, A>) -> bool {
        RcEqIdent::eq(self, other)
    }
}

impl<T: Eq + PSafe + ?Sized, A: MemPool> Eq for Parc<T, A> {}

impl<T: PartialOrd + PSafe + ?Sized, A: MemPool> PartialOrd for Parc<T, A> {
    #[inline(always)]
    fn partial_cmp(&self, other: &Parc<T, A>) -> Option<Ordering> {
        (**self).partial_cmp(&**other)
    }

    #[inline(always)]
    fn lt(&self, other: &Parc<T, A>) -> bool {
        **self < **other
    }

    #[inline(always)]
    fn le(&self, other: &Parc<T, A>) -> bool {
        **self <= **other
    }

    #[inline(always)]
    fn gt(&self, other: &Parc<T, A>) -> bool {
        **self > **other
    }

    #[inline(always)]
    fn ge(&self, other: &Parc<T, A>) -> bool {
        **self >= **other
    }
}

impl<T: Ord + PSafe + ?Sized, A: MemPool> Ord for Parc<T, A> {
    #[inline]
    fn cmp(&self, other: &Parc<T, A>) -> Ordering {
        (**self).cmp(&**other)
    }
}

impl<T: Hash + PSafe, A: MemPool> Hash for Parc<T, A> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        (**self).hash(state);
    }
}

impl<T: fmt::Display + PSafe, A: MemPool> fmt::Display for Parc<T, A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&**self, f)
    }
}

impl<T: fmt::Debug + PSafe, A: MemPool> fmt::Debug for Parc<T, A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.deref().fmt(f)
    }
}

impl<T: PSafe + ?Sized, A: MemPool> fmt::Pointer for Parc<T, A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Pointer::fmt(&(&**self as *const T), f)
    }
}

/// `Weak` is a version of [`Parc`] that holds a non-owning reference to the
/// managed allocation. The allocation is accessed by calling [`upgrade`] on the
/// `Weak` pointer, which returns an [`Option`]`<`[`Parc`]`<T>>`.
///
/// Since a `Weak` reference does not count towards ownership, it will not
/// prevent the value stored in the allocation from being dropped, and `Weak`
/// itself makes no guarantees about the value still being present. Thus it may
/// return [`None`] when [`upgrade`]d. Note however that a `Weak` reference
/// *does* prevent the allocation itself (the backing store) from being
/// deallocated.
///
/// A `Weak` pointer is useful for keeping a temporary reference to the
/// allocation managed by [`Parc`] without preventing its inner value from being
/// dropped. It is also used to prevent circular references between [`Parc`]
/// pointers, since mutual owning references would never allow either [`Parc`]
/// to be dropped. For example, a tree could have strong [`Parc`] pointers from
/// parent nodes to children, and `Weak` pointers from children back to their
/// parents.
///
/// The typical way to obtain a `Weak` pointer is to call [`Parc::downgrade`].
///
/// [`Parc::downgrade`]: ./struct.Parc.html#method.downgrade
/// [`upgrade`]: #method.upgrade
pub struct Weak<T: PSafe + ?Sized, A: MemPool> {
    ptr: Ptr<ParcInner<T, A>, A>,
}

impl<T: ?Sized, A: MemPool> !TxOutSafe for Weak<T, A> {}
impl<T: ?Sized, A: MemPool> !Sync for Weak<T, A> {}
impl<T: ?Sized, A: MemPool> !Send for Weak<T, A> {}
impl<T: ?Sized, A: MemPool> !VSafe for Weak<T, A> {}

impl<T: PSafe, A: MemPool> Weak<T, A> {
    pub fn as_raw(&self) -> *const T {
        match self.inner() {
            None => std::ptr::null(),
            Some(inner) => {
                let offset = data_offset_sized::<T, A>();
                let ptr = inner as *const ParcInner<T, A>;
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
            let fake_ptr = ptr as *mut ParcInner<T, A>;
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

    fn is_dangling(&self) -> bool {
        self.ptr.is_dangling()
    }

    /// Attempts to upgrade the `Weak` pointer to an [`Parc`], delaying
    /// dropping of the inner value if successful.
    ///
    /// Returns [`None`] if the inner value has since been dropped.
    ///
    /// # Examples
    ///
    /// ```
    /// use corundum::alloc::*;
    /// use corundum::sync::Parc;
    ///
    /// Heap::transaction(|j| {
    ///     let five = Parc::new(5, j);
    ///     let weak_five = Parc::downgrade(&five, j);
    ///     let strong_five = weak_five.upgrade(j);
    ///     assert!(strong_five.is_some());
    ///     
    ///     // Destroy all strong pointers.
    ///     drop(strong_five);
    ///     drop(five);
    ///     
    ///     assert!(weak_five.upgrade(j).is_none());
    /// }).unwrap()
    /// ```
    pub fn upgrade(&self, _journal: &Journal<A>) -> Option<Parc<T, A>> {
        // We use a CAS loop to increment the strong count instead of a
        // fetch_add because once the count hits 0 it must never be above 0.
        let inner = self.inner()?;

        // Relaxed load because any write of 0 that we can observe
        // leaves the field in a permanently zero state (so a
        // "stale" read of 0 is fine), and any other value is
        // confirmed via the CAS below.
        let mut n = inner.counter.strong.load(Relaxed);

        loop {
            if n == 0 {
                return None;
            }

            // See comments in `Arc::clone` for why we do this (for `mem::forget`).
            if n > MAX_REFCOUNT {
                std::process::abort();
            }

            #[cfg(not(feature = "no_log_rc"))]
            inner.log_count(_journal);

            // Relaxed is valid for the same reason it is on Arc's Clone impl
            match inner
                .counter
                .strong
                .compare_exchange_weak(n, n + 1, Relaxed, Relaxed)
            {
                Ok(_) => return Some(Parc::from_inner(self.ptr)), // null checked above
                Err(old) => n = old,
            }
        }
    }

    /// Gets the number of strong (`Parc`) pointers pointing to this allocation.
    ///
    /// If `self` was created using [`Weak::new`], this will return 0.
    pub fn strong_count(&self) -> usize {
        if let Some(inner) = self.inner() {
            inner.counter.strong.load(SeqCst)
        } else {
            0
        }
    }

    /// Gets an approximation of the number of `Weak` pointers pointing to this
    /// allocation.
    ///
    /// If `self` was created using [`Weak::new`], or if there are no remaining
    /// strong pointers, this will return 0.
    ///
    /// # Accuracy
    ///
    /// Due to implementation details, the returned value can be off by 1 in
    /// either direction when other threads are manipulating any `Parc`s or
    /// `Weak`s pointing to the same allocation.
    pub fn weak_count(&self) -> usize {
        self.inner()
            .map(|inner| {
                let weak = inner.counter.weak.load(SeqCst);
                let strong = inner.counter.strong.load(SeqCst);
                if strong == 0 {
                    0
                } else {
                    // Since we observed that there was at least one strong pointer
                    // after reading the weak count, we know that the implicit weak
                    // reference (present whenever any strong references are alive)
                    // was still around when we observed the weak count, and can
                    // therefore safely subtract it.
                    weak - 1
                }
            })
            .unwrap_or(0)
    }

    #[inline]
    fn inner(&self) -> Option<&ParcInner<T, A>> {
        if self.ptr.is_dangling() {
            None
        } else {
            Some(self.ptr.get_mut())
        }
    }

    /// Returns `true` if the two `Weak`s point to the same allocation (similar to
    /// [`std::ptr::eq`]), or if both don't point to any allocation
    /// (because they were created with `Weak::new()`).
    ///
    /// # Notes
    ///
    /// Since this compares pointers it means that `Weak::new()` will equal each
    /// other, even though they don't point to any allocation.
    ///
    #[inline]
    pub fn ptr_eq(&self, other: &Self) -> bool {
        self.ptr == other.ptr
    }
}

impl<T: PSafe + ?Sized, A: MemPool> Drop for Weak<T, A> {
    fn drop(&mut self) {
        if let Some(_inner) = self.inner() {
            #[cfg(not(feature = "no_log_rc"))]
            {
                let journal = Journal::<A>::current(true).unwrap();
                _inner.log_count(journal.0);
            }

            // If we find out that we were the last weak pointer, then its time to
            // deallocate the data entirely. See the discussion in Arc::drop() about
            // the memory orderings
            //
            // It's not necessary to check for the locked state here, because the
            // weak count can only be locked if there was precisely one weak ref,
            // meaning that drop could only subsequently run ON that remaining weak
            // ref, which can only happen after the lock is released.
            let inner = if let Some(inner) = self.inner() {
                inner
            } else {
                return;
            };

            if inner.counter.weak.fetch_sub(1, Release) == 1 {
                atomic::fence(Acquire);
                unsafe {
                    A::free(self.ptr.as_mut());
                }
            }
        }
    }
}

impl<T: PSafe + ?Sized, A: MemPool> PClone<A> for Weak<T, A> {
    #[inline]
    fn pclone(&self, _journal: &Journal<A>) -> Weak<T, A> {
        let inner = if let Some(inner) = self.inner() {
            inner
        } else {
            return Weak { ptr: self.ptr };
        };

        #[cfg(not(feature = "no_log_rc"))]
        inner.log_count(_journal);

        // See comments in Arc::clone() for why this is relaxed.  This can use a
        // fetch_add (ignoring the lock) because the weak count is only locked
        // where are *no other* weak pointers in existence. (So we can't be
        // running this code in that case).
        let old_size = inner.counter.weak.fetch_add(1, Relaxed);

        // See comments in Arc::clone() for why we do this (for mem::forget).
        if old_size > MAX_REFCOUNT {
            std::process::abort();
        }

        Weak { ptr: self.ptr }
    }
}

impl<T: PSafe + ?Sized + fmt::Debug, A: MemPool> fmt::Debug for Weak<T, A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "(Weak)")
    }
}

impl<T: PSafe + ?Sized, A: MemPool> Default for Weak<T, A> {
    fn default() -> Self {
        Weak::new()
    }
}

trait ParcBoxPtr<T: PSafe + ?Sized, A: MemPool> {
    fn count(&self) -> &Counter;

    #[inline]
    #[cfg(not(feature = "no_log_rc"))]
    fn log_count(&self, journal: *const Journal<A>) {
        let inner = self.count();

        unsafe {
            if A::contains(inner as *const _ as *const u8 as u64) {
                let flag = &inner.has_log as *const u8 as *mut u8;
                if std::intrinsics::atomic_cxchg_acqrel(flag, 0, 1).0 == 0 {
                    inner.take_log(&*journal, Notifier::Atomic(Ptr::from_ref(&inner.has_log)));
                }
            }
        }
    }
}

impl<T: PSafe + ?Sized, A: MemPool> ParcBoxPtr<T, A> for Parc<T, A> {
    #[inline(always)]
    fn count(&self) -> &Counter {
        &self.ptr.counter
    }
}

impl<T: PSafe + ?Sized, A: MemPool> ParcBoxPtr<T, A> for ParcInner<T, A> {
    #[inline(always)]
    fn count(&self) -> &Counter {
        &self.counter
    }
}

impl<T: PSafe + ?Sized, A: MemPool> borrow::Borrow<T> for Parc<T, A> {
    fn borrow(&self) -> &T {
        &self.inner().value
    }
}

impl<T: PSafe + ?Sized, A: MemPool> AsRef<T> for Parc<T, A> {
    fn as_ref(&self) -> &T {
        &self.inner().value
    }
}

impl<T: PSafe + ?Sized, A: MemPool> Unpin for Parc<T, A> {}

unsafe fn data_offset<T, A: MemPool>(ptr: *const T) -> isize {
    data_offset_align::<A>(mem::align_of_val(&*ptr))
}

fn data_offset_sized<T, A: MemPool>() -> isize {
    data_offset_align::<A>(mem::align_of::<T>())
}

#[inline]
fn data_offset_align<A: MemPool>(align: usize) -> isize {
    let layout = std::alloc::Layout::new::<ParcInner<(), A>>();
    (layout.size() + layout.padding_needed_for(align)) as isize
}

/// `VWeak` is a version of [`Parc`] that holds a non-owning thread-safe 
/// reference to the managed allocation in the demote heap. The allocation is
/// accessed by calling [`upgrade`] on the `VWeak` pointer, which returns an
/// [`Option`]`<`[`Parc`]`<T>>`.
///
/// Since a `VWeak` reference does not count towards ownership, it will not
/// prevent the value stored in the allocation from being dropped, and `VWeak`
/// itself makes no guarantees about the value still being present. Thus it may
/// return [`None`] when [`upgrade`]d. Note however that a `VWeak` reference,
/// unlike [`Weak`], *does NOT* prevent the allocation itself (the backing
/// store) from being deallocated.
///
/// A `VWeak` pointer is useful for keeping a temporary thread-safe reference to
/// the persistent allocation managed by [`Parc`] without preventing its inner
/// value from being dropped.
///
/// The typical way to obtain a `VWeak` pointer is to call [`Parc::demote`].
///
/// [`Parc::demote`]: ./struct.Parc.html#method.demote
/// [`upgrade`]: #method.upgrade
pub struct VWeak<T: ?Sized, A: MemPool> {
    ptr: *const ParcInner<T, A>,
    valid: *mut VWeakValid,
    gen: u32,
}

impl<T: ?Sized, A: MemPool> UnwindSafe for VWeak<T, A> {}
impl<T: ?Sized, A: MemPool> RefUnwindSafe for VWeak<T, A> {}
unsafe impl<T: PSend + ?Sized, A: MemPool> Send for VWeak<T, A> {}
unsafe impl<T: PSend + ?Sized, A: MemPool> Sync for VWeak<T, A> {}
unsafe impl<T: ?Sized, A: MemPool> TxInSafe for VWeak<T, A> {}
unsafe impl<T: ?Sized, A: MemPool> TxOutSafe for VWeak<T, A> {}
unsafe impl<T: ?Sized, A: MemPool> PSafe for VWeak<T, A> {}

impl<T: PSafe + ?Sized, A: MemPool> VWeak<T, A> {
    fn new(parc: &Parc<T, A>) -> VWeak<T, A> {
        let list = parc.ptr.vlist.as_mut();
        VWeak {
            ptr: parc.ptr.as_ptr(),
            valid: list.append(),
            gen: A::gen(),
        }
    }

    /// Attempts to promote the `VWeak` pointer to an [`Parc`], delaying
    /// dropping of the inner value if successful.
    ///
    /// Returns [`None`] if the inner value has since been dropped.
    ///
    /// # Examples
    ///
    /// ```
    /// use corundum::default::*;
    /// use std::mem::drop;
    /// 
    /// type P = BuddyAlloc;
    /// let obj = P::open::<Root>("foo.pool", O_CF).unwrap();
    ///
    /// struct Root(PRefCell<Option<Parc<i32>>>);
    /// impl RootObj<P> for Root {
    ///     fn init(j: &Journal) -> Self {
    ///         Root(PRefCell::new(Some(Parc::new(10, j)),j))
    ///     }
    /// }
    ///
    /// let vweak_obj = obj.0.borrow().as_ref().unwrap().demote();
    /// 
    /// P::transaction(|j| {
    ///     let strong_obj = vweak_obj.promote(j);
    ///     assert!(strong_obj.is_some());
    ///     
    ///     // Destroy all strong pointers.
    ///     drop(strong_obj);
    ///     *obj.0.borrow_mut(j) = None; // RootCell does not drop, so make it None
    /// 
    ///     assert!(vweak_obj.promote(j).is_none());
    /// }).unwrap();
    /// ```
    pub fn promote(&self, _journal: &Journal<A>) -> Option<Parc<T, A>> {
        // We use a CAS loop to increment the strong count instead of a
        // fetch_add as this function should never take the reference count
        // from zero to one.
        let inner = self.inner()?;

        // Relaxed load because any write of 0 that we can observe
        // leaves the field in a permanently zero state (so a
        // "stale" read of 0 is fine), and any other value is
        // confirmed via the CAS below.
        let mut n = inner.counter.strong.load(Relaxed);

        loop {
            if n == 0 {
                return None;
            }

            // See comments in `Arc::clone` for why we do this (for `mem::forget`).
            if n > MAX_REFCOUNT {
                std::process::abort();
            }

            #[cfg(not(feature = "no_log_rc"))]
            inner.log_count(_journal);

            // Relaxed is fine for the failure case because we don't have any expectations about the new state.
            // Acquire is necessary for the success case to synchronise with `Arc::new_cyclic`, when the inner
            // value can be initialized after `Weak` references have already been created. In that case, we
            // expect to observe the fully initialized value.
            match inner.counter.strong.compare_exchange_weak(n, n + 1, Acquire, Relaxed) {
                Ok(_) => return Some(Parc::from_inner(Ptr::from_raw(self.ptr))), // null checked above
                Err(old) => n = old,
            }
        }
    }

    #[inline]
    fn inner(&self) -> Option<&ParcInner<T, A>> {
        unsafe {
            if !(*self.valid).valid.load(Acquire) || self.gen != A::gen() {
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
                if (*self.valid).valid.load(Acquire) {
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
            if !this.list.is_null() {
                let mut head = match (*this.list).head.lock() {
                    Ok(g) => g,
                    Err(p) => p.into_inner(),
                };
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

struct VWeakValid {
    valid: AtomicBool,
    next: *mut VWeakValid,
    prev: *mut VWeakValid,
    list: *mut VWeakList,
}

use std::sync::Mutex as StdMutex;

struct VWeakList {
    head: StdMutex<*mut VWeakValid>,
}

impl VWeakList {
    fn append(&mut self) -> *mut VWeakValid {
        let list = self as *mut Self;
        let mut head = match self.head.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        let new = Box::into_raw(Box::new(VWeakValid {
            valid: AtomicBool::new(true),
            next: *head,
            prev: std::ptr::null_mut(),
            list,
        }));
        if !(*head).is_null() {
            unsafe {
                (**head).prev = new;
            }
        }
        *head = new;
        new
    }
}

impl Default for VWeakList {
    fn default() -> Self {
        VWeakList {
            head: StdMutex::new(std::ptr::null_mut()),
        }
    }
}

impl Drop for VWeakList {
    fn drop(&mut self) {
        let head = match self.head.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        unsafe {
            let mut curr = *head;
            while !curr.is_null() {
                (*curr).valid.store(false, Release);
                (*curr).list = std::ptr::null_mut();
                curr = (*curr).next;
            }
        }
    }
}
