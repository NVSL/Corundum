use crate::result::Result;
use crate::cell::LazyCell;
use crate::{TxInSafe, TxOutSafe, utils};
use std::collections::hash_map::HashMap;
use std::fmt::{self, Debug};
use std::fs::OpenOptions;
use std::io::{self, Error, Write};
use std::panic::{RefUnwindSafe, UnwindSafe};
use std::path::Path;
use std::sync::Mutex;
use std::thread::ThreadId;
use std::{mem, panic, ptr, slice, str, thread};

const MAX_TRANS: usize = 4096;


/// A third-party observer for multi-pool transactions 
///
/// It provides an atomic supper transaction (a [`session`]) for manipulating
/// persistent data in multiple pools, atomically. The involved pools go to a
/// transient state when they call transaction inside a chaperoned [`session`].
/// The finalization functions (e.g. [`commit`] or [`rollback`]) are delayed
/// until the end of the [`session`]. To keep track of pools' states, it creates
/// a chaperon file with necessary information for recovering them, in case of a
/// crash.
/// 
/// [`session`]: #method.session
/// [`commit`]: ../alloc/trait.MemPool.html#method.commit
/// [`rollback`]: ../alloc/trait.MemPool.html#method.rollback
pub struct Chaperon {
    len: usize,
    completed: bool,
    done: [bool; MAX_TRANS],
    filename: [u8; 4096],
    filename_len: usize,
    vdata: Option<VData>,
}

struct VData {
    mmap: memmap::MmapMut,
    delayed_commit: HashMap<ThreadId, Vec<unsafe fn() -> ()>>,
    delayed_rollback: HashMap<ThreadId, Vec<unsafe fn() -> ()>>,
    delayed_clear: HashMap<ThreadId, Vec<unsafe fn() -> ()>>,
    mutex: u8,
}

impl VData {
    pub fn new(mmap: memmap::MmapMut) -> Self {
        Self {
            mmap,
            delayed_commit: HashMap::new(),
            delayed_rollback: HashMap::new(),
            delayed_clear: HashMap::new(),
            mutex: 0,
        }
    }
}

impl !TxOutSafe for Chaperon {}
impl UnwindSafe for Chaperon {}
impl RefUnwindSafe for Chaperon {}
unsafe impl TxInSafe for Chaperon {}
unsafe impl Send for Chaperon {}
unsafe impl Sync for Chaperon {}

struct SyncBox<T: ?Sized> {
    data: *mut T
}

impl<T: ?Sized> SyncBox<T> {
    fn new(data: *mut T) -> Self {
        Self { data }
    }

    fn get(&self) -> *mut T {
        self.data
    }
}

unsafe impl<T:?Sized> Sync for SyncBox<T> {}
unsafe impl<T:?Sized> Send for SyncBox<T> {}

static mut CLIST: LazyCell<Mutex<HashMap<ThreadId, SyncBox<Chaperon>>>> = 
    LazyCell::new(|| Mutex::new(HashMap::new()));

fn new_chaperon(filename: &str) -> Result<*mut Chaperon> {
    let mut clist = match unsafe { CLIST.lock() } {
        Ok(g) => g,
        Err(p) => p.into_inner()
    };
    let tid = thread::current().id();
    if clist.contains_key(&tid) {
        return Err("Another chaperoned transaction is open".to_string());
    }
    let c = Chaperon::new(filename.to_string())
        .expect(&format!("could not create chaperon file `{}`", filename));
    clist.entry(tid).or_insert(SyncBox::new(c));
    Ok(clist.get(&tid).unwrap().get())
}

fn drop_chaperon() {
    let mut clist = match unsafe { CLIST.lock() } {
        Ok(g) => g,
        Err(p) => p.into_inner()
    };
    let tid = thread::current().id();
    clist.remove(&tid);
}

fn current_chaperon() -> Option<*mut Chaperon> {
    let clist = match unsafe { CLIST.lock() } {
        Ok(g) => g,
        Err(p) => p.into_inner()
    };
    let tid = thread::current().id();
    if clist.contains_key(&tid) {
        Some(clist.get(&tid).unwrap().get())
    } else {
        None
    }
}

impl Chaperon {
    pub(crate) fn new(filename: String) -> io::Result<&'static mut Self> {
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            // Note: If file already exists, it may reflect three cases
            //   1. A crash happened after creating the file but before assigning
            //      it to a journal.
            //   2. User mistakenly specified a filename which already exists
            //   3. Crash after assigning the journals
            // In the first case, it is safe to delete the file, but we can't do
            // it here because the second case is more common and we don't want
            // to delete a file when it might be required by another chaperoned
            // session.
            .open(&filename)?;
        file.set_len(1024 * 1024 * 1)?;
        let mut a = Self {
            len: 0,
            completed: false,
            done: [true; MAX_TRANS],
            filename: [0; 4096],
            filename_len: filename.len(),
            vdata: None,
        };
        let bytes = filename.as_bytes();
        for i in 0..4096.min(filename.len()) {
            a.filename[i] = bytes[i];
        }
        file.write_all(a.as_bytes()).unwrap();
        unsafe { Self::load(&filename) }
    }

    fn deref(raw: *mut u8) -> &'static mut Self {
        unsafe { &mut *utils::read(raw) }
    }

    fn as_bytes(&self) -> &[u8] {
        let ptr = self as *const Self;
        let ptr = ptr as *const u8;
        unsafe { std::slice::from_raw_parts(ptr, std::mem::size_of::<Self>()) }
    }

    /// Loads a chaperon file
    pub unsafe fn load(filename: &str) -> io::Result<&'static mut Self> {
        if Path::new(&filename).exists() {
            let file = OpenOptions::new().read(true).write(true).open(&filename)?;
            let mut mmap = memmap::MmapOptions::new().map_mut(&file).unwrap();
            let slf = Self::deref(mmap.get_mut(0).unwrap());
            mem::forget(ptr::replace(&mut slf.vdata, Some(VData::new(mmap))));
            Ok(slf)
        } else {
            Err(Error::last_os_error())
        }
    }

    pub(crate) fn current() -> Option<*mut Chaperon> {
        current_chaperon()
    }

    pub(crate) fn new_section(&mut self) -> usize {
        use crate::ll::persist_obj;

        assert!(self.len < MAX_TRANS, "reached max number of attachments");
        self.len += 1;
        self.done[self.len - 1] = false;
        persist_obj(self, true);
        self.len
    }

    #[inline]
    pub(crate) fn is_done(&self, id: usize) -> bool {
        let id = id - 1;
        assert!(id < self.len, "index out of range");
        self.done[id]
    }

    #[inline]
    pub(crate) fn finish(&mut self, id: usize) {
        let id = id - 1;
        assert!(id < self.len, "index out of range");
        self.done[id] = true;
        if self.completed() {
            self.close();
        }
    }

    pub(crate) fn completed(&mut self) -> bool {
        if self.completed {
            true
        } else {
            for i in 0..self.len {
                if !self.done[i] {
                    return false;
                }
            }
            self.completed = true;
            true
        }
    }

    fn close(&self) {
        // std::fs::remove_file(self.filename()).unwrap();
    }

    /// Returns the chaperon filename
    pub fn filename(&self) -> &str {
        unsafe {
            let slice = slice::from_raw_parts(&self.filename[0], self.filename_len);
            str::from_utf8(slice).unwrap()
        }
    }

    pub(crate) fn postpone(
        &mut self,
        commit: unsafe fn()->(),
        rollback: unsafe fn()->(),
        clear: unsafe fn()->(),
    ) {
        if let Some(vdata) = self.vdata.as_mut() {
            let tid = thread::current().id();
            let commits = vdata.delayed_commit.entry(tid).or_insert(Vec::new());
            let rollbacks = vdata.delayed_rollback.entry(tid).or_insert(Vec::new());
            let clears = vdata.delayed_clear.entry(tid).or_insert(Vec::new());
            commits.push(commit);
            rollbacks.push(rollback);
            clears.push(clear);
        }
    }

    fn execute_delayed_commits(&mut self) {
        if let Some(vdata) = self.vdata.as_mut() {
            let tid = thread::current().id();
            let commits = vdata.delayed_commit.entry(tid).or_insert(Vec::new());
            let clears = vdata.delayed_clear.entry(tid).or_insert(Vec::new());
            for commit in commits {
                unsafe { commit(); }
            }
            self.completed = true;
            for clear in clears {
                unsafe { clear(); }
            }
            vdata.delayed_commit.remove(&tid);
            vdata.delayed_clear.remove(&tid);
        }
        // self.close();
    }

    fn execute_delayed_rollbacks(&mut self) {
        if let Some(vdata) = self.vdata.as_mut() {
            let tid = thread::current().id();
            let rollbacks = vdata.delayed_rollback.entry(tid).or_insert(Vec::new());
            let clears = vdata.delayed_clear.entry(tid).or_insert(Vec::new());
            for rollback in rollbacks {
                unsafe { rollback(); }
            }
            self.completed = true;
            for clear in clears {
                unsafe { clear(); }
            }
            vdata.delayed_rollback.remove(&tid);
            vdata.delayed_clear.remove(&tid);
        }
        // self.close();
    }

    #[inline]
    /// Starts a chaperoned session
    /// 
    /// It creates a chaperoned session in which multiple pools can start a
    /// [`transaction`]. The transactions won't be finalized until the session
    /// ends. A chaperon file keeps the necessary information for recovering the
    /// involved pools. If the operation is successful, it returns a value of
    /// type `T`.
    /// 
    /// # Safety
    /// 
    /// * In case of a crash, the involved pools are not individually
    /// recoverable on the absence of the chaperon file.
    /// * Chaperoned sessions cannot be nested.
    /// 
    /// # Examples
    ///
    /// ```
    /// use corundum::alloc::heap::*;
    /// use corundum::stm::{Chaperon, Journal};
    /// use corundum::cell::{PCell, RootObj};
    /// use corundum::boxed::Pbox;
    ///
    /// corundum::pool!(pool1);
    /// corundum::pool!(pool2);
    ///
    /// type P1 = pool1::BuddyAlloc;
    /// type P2 = pool2::BuddyAlloc;
    ///
    /// struct Root<M: MemPool> {
    ///     val: Pbox<PCell<i32, M>, M>
    /// }
    ///
    /// impl<M: MemPool> RootObj<M> for Root<M> {
    ///     fn init(j: &Journal<M>) -> Self {
    ///         Root { val: Pbox::new(PCell::new(0), j) }
    ///     }
    /// }
    ///
    /// let root1 = P1::open::<Root<P1>>("pool1.pool", O_CF).unwrap();
    /// let root2 = P2::open::<Root<P2>>("pool2.pool", O_CF).unwrap();
    ///
    /// let _=Chaperon::session("chaperon.pool", || {
    ///     let v = P2::transaction(|j| {
    ///         let old = root2.val.get();
    ///         root2.val.set(old+1, j); // <-- should persist if both transactions commit
    ///         old // <-- Send out p2's old data
    ///     }).unwrap();
    ///     P1::transaction(|j| {
    ///         let mut p1 = root1.val.get();
    ///         root1.val.set(p1+v, j);
    ///     }).unwrap();
    /// }).unwrap(); // <-- both transactions commit here
    ///
    /// let v1 = root1.val.get();
    /// let v2 = root2.val.get();
    /// println!("root1 = {}", v1);
    /// println!("root2 = {}", v2);
    /// assert_eq!(v1, calc(v2-1));
    ///
    /// fn calc(n: i32) -> i32 {
    ///     if n < 1 {
    ///         0
    ///     } else {
    ///         n + calc(n-1)
    ///     }
    /// }
    /// ```
    /// 
    /// [`transaction`]: ./fn.transaction.html
    pub fn session<T, F: FnOnce() -> T>(filename: &str, body: F) -> Result<T>
    where
        F: panic::UnwindSafe,
        T: panic::UnwindSafe + TxOutSafe,
    {
        let chaperon = unsafe { &mut *new_chaperon(filename)? };
        let res = panic::catch_unwind(|| body());
        if let Ok(res) = res {
            chaperon.execute_delayed_commits();
            drop_chaperon();
            Ok(res)
        } else {
            chaperon.execute_delayed_rollbacks();
            drop_chaperon();
            Err("Unsuccessful chaperoned transaction".to_string())
        }
    }
}

impl Drop for Chaperon {
    fn drop(&mut self) {
        if let Some(vdata) = self.vdata.as_ref() {
            vdata.mmap.flush().unwrap();
        }
    }
}

impl Debug for Chaperon {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{{ filename: {}, len: {}, [", self.filename(), self.len)?;
        for i in 0..self.len {
            write!(f, "{}{}", if i == 0 { "" } else { ", " }, self.done[i])?;
        }
        write!(f, "] }}")
    }
}
