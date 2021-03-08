//! The journal object for keeping logs
use crate::alloc::MemPool;
use crate::ll::*;
use crate::ptr::Ptr;
use crate::stm::*;
use crate::*;
use std::collections::HashMap;
use std::fmt::{self, Debug, Formatter};


#[cfg(all(feature = "use_pspd", feature = "use_vspd"))]
compile_error!("Cannot use both volatile and persistent scratchpad");

#[cfg(all(feature = "use_pspd", not(feature = "use_vspd")))]
use crate::stm::pspd::Scratchpad;

#[cfg(all(feature = "use_vspd", not(feature = "use_pspd")))]
use crate::stm::vspd::Scratchpad;

/// Determines that the changes are committed
pub const JOURNAL_COMMITTED: u64 = 0x0000_0001;

/// A Journal object to be used for writing logs onto
///
/// Each transaction, hence each thread, may have only one journal for every
/// memory pool to write the logs. The journal itself resides in a pool.
/// Journals are linked together in the `MemPool` object to be accessible in
/// recovery procedure.
///
/// It is not allowed to create a `Journal` object. However, [`transaction()`]
/// creates a journal at the beginning and passes a reference to it to the body
/// closure. So, to obtain a reference to a `Journal`, you may wrap a
/// transaction around your code. For example:
///
/// ```
/// use corundum::alloc::heap::*;
///
/// let cell = Heap::transaction(|journal| {
///     let cell = Pbox::new(PCell::new(10), journal);
/// 
///     assert_eq!(cell.get(), 10);
/// }).unwrap();
/// ```
/// 
/// A `Journal` consists of one or more `page`s. A `page` provides a fixed
/// number of log slots which is specified by `PAGE_SIZE` (64). This helps
/// performance as the logs are pre-allocated. When the number of logs in a page
/// exceeds 64, `Journal` object atomically allocate a new page for another 64
/// pages before running the operations.
///
/// `Journal`s by default are deallocated after the transaction or recovery.
/// However, it is possible to pin journals in the pool if they are used
/// frequently by enabling "pin_journals" feature.
/// 
/// [`transaction()`]: ./fn.transaction.html
/// 
pub struct Journal<A: MemPool> {
    pages: Ptr<Page<A>, A>,

    #[cfg(feature = "pin_journals")]
    current: Ptr<Page<A>, A>,

    #[cfg(any(feature = "use_pspd", feature = "use_vspd"))]
    spd: Scratchpad<A>,

    gen: u32,
    flags: u64,
    sec_id: u64,
    prev_off: u64,
    next_off: u64,
    chaperon: [u8;64],
}

impl<A: MemPool> !PSafe for Journal<A> {}
impl<A: MemPool> !Send for Journal<A> {}
impl<A: MemPool> !Sync for Journal<A> {}
impl<A: MemPool> !TxOutSafe for Journal<A> {}
impl<A: MemPool> !TxInSafe for Journal<A> {}
impl<A: MemPool> !LooseTxInUnsafe for Journal<A> {}
impl<A: MemPool> !std::panic::RefUnwindSafe for Journal<A> {}
impl<A: MemPool> !std::panic::UnwindSafe for Journal<A> {}

#[derive(Clone, Copy)]
struct Page<A: MemPool> {
    len: usize,
    head: usize,
    next: Ptr<Page<A>, A>,
    logs: [Log<A>; PAGE_LOG_SLOTS],
}

impl<A: MemPool> Page<A> {
    #[inline]
    /// Writes a new log to the journal
    fn write(&mut self, log: LogEnum, notifier: Notifier<A>) -> Ptr<Log<A>, A> {
        #[cfg(not(feature = "use_ntstore"))] {
            self.logs[self.len] = Log::new(log, notifier);
        }
        #[cfg(feature = "use_ntstore")] unsafe {
            std::intrinsics::nontemporal_store(&mut self.logs[self.len], Log::new(log, notifier));
        }
        persist(&self.logs[self.len], std::mem::size_of::<Log<A>>(), false);

        let log = unsafe { Ptr::new_unchecked(&self.logs[self.len]) };
        self.len += 1;
        log
    }

    #[inline]
    fn is_full(&self) -> bool {
        self.len == PAGE_LOG_SLOTS
    }

    unsafe fn notify(&mut self) {
        for i in 0..self.len {
            self.logs[i].notify(0);
        }
    }

    unsafe fn commit(&mut self) {
        for i in 0..self.len {
            self.logs[i].commit();
        }
    }

    unsafe fn rollback(&mut self) {
        for i in 0..self.len {
            self.logs[i].rollback();
        }
    }

    unsafe fn recover(&mut self, rollback: bool) {
        for i in 0..self.len {
            self.logs[self.len - i - 1].recover(rollback);
        }
    }

    unsafe fn ignore(&mut self) {
        self.len = 0;
        self.head = 0;
        self.logs = [Default::default(); PAGE_LOG_SLOTS];
    }

    unsafe fn clear(&mut self) {
        for i in self.head..self.len {
            self.logs[i].clear();
            self.head += 1;
        }

        #[cfg(feature = "pin_journals")]
        {
            self.head = 0;
            self.len = 0;
        }
    }

    fn into_iter(&self) -> std::vec::IntoIter<Log<A>> {
        Vec::from(self.logs).into_iter()
    }
}

impl<A: MemPool> Debug for Page<A> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln!(f, "LOGS:")?;
        for i in 0..self.len {
            writeln!(f, "    {:?}", self.logs[i])?;
        }
        Ok(())
    }
}

impl<A: MemPool> Journal<A> {
    /// Create new `Journal` with default values
    pub unsafe fn new(gen: u32) -> Self {
        Self {
            pages: Ptr::dangling(),

            #[cfg(feature = "pin_journals")]
            current: Ptr::dangling(),

            #[cfg(any(feature = "use_pspd", feature = "use_vspd"))]
            spd: Scratchpad::new(),

            gen,
            flags: 0,
            sec_id: 0,
            next_off: u64::MAX,
            prev_off: u64::MAX,
            chaperon: [0; 64],
        }
    }

    /// Returns the generation number of this journal
    pub fn gen(&self) -> u32 {
        self.gen
    }

    /// Returns true if the journal is committed
    pub fn is_committed(&self) -> bool {
        self.is_set(JOURNAL_COMMITTED)
    }

    /// Sets a flag
    pub(crate) fn set(&mut self, flag: u64) {
        self.flags |= flag;
        persist_obj(&self.flags, true);
    }

    /// Resets a flag
    pub(crate) fn unset(&mut self, flag: u64) {
        self.flags &= !flag;
    }

    /// Checks a flag
    pub fn is_set(&self, flag: u64) -> bool {
        self.flags & flag == flag
    }

    /// Atomically enters into the list journals of the owner pool
    pub unsafe fn enter_into(&mut self, head_off: &u64, zone: usize) {
        let me = A::off_unchecked(self);
        self.next_off = *head_off;
        A::log64(A::off_unchecked(head_off), me, zone);

        if let Ok(j) = A::deref_mut::<Journal<A>>(*head_off) {
            A::log64(A::off_unchecked(&j.prev_off), me, zone);
        }
    }

    #[inline]
    #[cfg(feature = "pin_journals")]
    fn next_page(&self, page: Ptr<Page<A>, A>) -> Ptr<Page<A>, A> {
        if page.is_dangling() {
            self.new_page()
        } else if page.is_full() {
            self.next_page(page.next)
        } else {
            page
        }
    }

    /// Writes a new log to the journal
    #[cfg(feature = "pin_journals")]
    pub(crate) fn write(&self, log: LogEnum, notifier: Notifier<A>) -> Ptr<Log<A>, A> {
        let mut page = self.next_page(self.current);
        page.as_mut().write(log, notifier)
    }

    #[inline]
    fn new_page(&self) -> Ptr<Page<A>, A> {
        #[cfg(feature = "stat_perf")]
        let _perf = crate::stat::Measure::<A>::NewPage(std::time::Instant::now());
        unsafe {
            let page = Page::<A> {
                len: 0,
                head: 0,
                next: self.pages,
                logs: [Default::default(); PAGE_LOG_SLOTS]
            };
            let (_, off, _, z) = A::atomic_new(page);
            A::log64(A::off_unchecked(self.pages.off_ref()), off, z);
            
            #[cfg(feature = "pin_journals")] {
                A::log64(A::off_unchecked(self.current.off_ref()), off, z);
                // eprintln!("new page for {:p} at {:x}", self as *const Self, off);
            }

            A::perform(z);

            self.pages
        }
    }

    /// Writes a new log to the journal
    #[cfg(not(feature = "pin_journals"))]
    pub(crate) fn write(&self, log: LogEnum, notifier: Notifier<A>) -> Ptr<Log<A>, A> {
        let mut page = if self.pages.is_dangling() {
            self.new_page()
        } else if self.pages.is_full() {
            self.new_page()
        } else {
            self.pages
        };
        page.as_mut().write(log, notifier)
    }

    /// Writes a new log to the journal
    #[cfg(feature = "pin_journals")]
    pub unsafe fn drop_pages(&mut self) {
        while let Some(page) = self.pages.clone().as_option() {
            let nxt = page.next;
            let z = A::pre_dealloc(page.as_mut_ptr() as *mut u8, std::mem::size_of::<Page<A>>());
            A::log64(A::off_unchecked(self.pages.off_ref()), nxt.off(), z);
            A::perform(z);
        }
        self.current = Ptr::dangling();
        self.pages = Ptr::dangling();
    }

    /// Writes a new log to the journal
    #[cfg(any(feature = "use_pspd", feature = "use_vspd"))]
    #[inline]
    pub(crate) fn draft<T: ?Sized>(&self, val: &T) -> *mut T {
        unsafe { utils::as_mut(self).spd.write(val, A::off(val).unwrap()) }
    }

    /// Returns a string containing the logging information
    pub fn recovery_info(&self, info_level: u32) -> String {
        let mut i = 1;
        let mut _cidx = 1;
        let mut log_cnt = HashMap::<String, u64>::new();
        let mut curr = self.pages;
        let mut pgs = vec![];
        while let Some(page) = curr.as_option() {
            if info_level > 2 {
                pgs.push(format!("  page {:<3} at offset {:x} (len = {}, full = {})", i, page.off(), page.len, page.is_full()));
            }

            #[cfg(feature = "pin_journals")] {
                if self.current == *page {
                    _cidx = i;
                }
            }

            for log in page.into_iter() {
                let entry = log_cnt.entry(log.kind()).or_default();
                *entry += 1;
                if info_level > 3 && log != LogEnum::None {
                    pgs.push(format!("    {:?}", log));
                }
            }

            i += 1;
            curr = page.next;
        }

        let mut total_logs = 0;
        let mut logs_indv = vec![];
        for (kind, count) in log_cnt {
            if kind != "None" {
                total_logs += count;
            }
            if info_level > 1 {
                logs_indv.push(format!("  {:<16} {}", kind, count));
            }
        }

        let mut res = format!("Committed: {}\n", 
            if self.is_committed() { "Yes" } else { "No" });
        res += &format!("Chaperoned session id: {}\n", self.sec_id);
        res += &format!("Chaperone file: {}\n", String::from_utf8(self.chaperon.to_vec()).unwrap_or("".to_string()));
        res += &format!("Number of pages: {}\n", i-1);

        #[cfg(feature = "pin_journals")] {
            res += &format!("current page at offset {:x} (index = {})", self.current.off(), _cidx);
        }

        res += &format!("Number of logs: {}\n", total_logs);
        if info_level > 1 {
            for ln in logs_indv {
                res += &format!("{}\n", ln);
            }
        }

        if info_level > 2 {
            res += "Contents:\n";
            for ln in pgs {
                res += &format!("{}\n", ln);
            }
        }

        res
    }

    /// Commits all logs in the journal
    pub unsafe fn commit(&mut self) {
        #[cfg(any(feature = "use_pspd", feature = "use_vspd"))] {
            self.spd.commit();
        }
        self.set(JOURNAL_COMMITTED);
        let mut curr = self.pages;
        while let Some(page) = curr.as_option() {
            page.notify();
            curr = page.next;
        }
        let mut curr = self.pages;
        while let Some(page) = curr.as_option() {
            page.commit();
            curr = page.next;
        }
    }

    /// Reverts all changes
    pub unsafe fn rollback(&mut self) {
        #[cfg(any(feature = "use_pspd", feature = "use_vspd"))] {
            self.spd.rollback();
        }
        let mut curr = self.pages;
        while let Some(page) = curr.as_option() {
            page.notify();
            curr = page.next;
        }
        let mut curr = self.pages;
        while let Some(page) = curr.as_option() {
            page.rollback();
            curr = page.next;
        }
        self.set(JOURNAL_COMMITTED);
    }

    /// Recovers from a crash or power failure
    pub unsafe fn recover(&mut self) {
        let mut curr = self.pages;
        while let Some(page) = curr.as_option() {
            page.notify();
            curr = page.next;
        }
        let mut curr = self.pages;
        let fast_forward = self.fast_forward();
        if !self.is_set(JOURNAL_COMMITTED) || fast_forward {
            let rollback = !fast_forward || !self.is_set(JOURNAL_COMMITTED);
            #[cfg(any(feature = "use_pspd", feature = "use_vspd"))] {
                if rollback {
                    self.spd.rollback();
                } else {
                    self.spd.recover();
                }
            }
            while let Some(page) = curr.as_option() {
                page.recover(rollback);
                curr = page.next;
            }
            self.set(JOURNAL_COMMITTED);
        }
    }

    /// Clears all logs and drops itself from the memory pool
    pub unsafe fn clear(&mut self) {
        #[cfg(any(feature = "use_pspd", feature = "use_vspd"))] {
            self.spd.clear();
        }
        #[cfg(feature = "pin_journals")]
        {
            let mut page = self.pages.as_option();
            while let Some(p) = page {
                p.clear();
                page = p.next.as_option();
            }
            self.current = self.pages;
        }

        #[cfg(not(feature = "pin_journals"))] {
            while let Some(page) = self.pages.clone().as_option() {
                let nxt = page.next;
                page.clear();
                let z = A::pre_dealloc(page.as_mut_ptr() as *mut u8, std::mem::size_of::<Page<A>>());
                A::log64(A::off_unchecked(self.pages.off_ref()), nxt.off(), z);
                A::perform(z);
            }
        }
        // if let Ok(prev) = A::deref_mut::<Self>(self.prev_off) {
        //     prev.next_off = self.next_off;
        // }
        // if let Ok(next) = A::deref_mut::<Self>(self.next_off) {
        //     next.prev_off = self.prev_off;
        // }
        self.complete();

        #[cfg(not(feature = "pin_journals"))] {
            A::drop_journal(self);
            A::journals(|journals| {
                journals.remove(&std::thread::current().id());
            });
        }
    }

    /// Determines whether to fast-forward or rollback the transaction
    /// on recovery according to the following table:
    ///
    /// ```text
    ///  ┌───────────┬────────────┬──────────┬─────┐
    ///  │ Committed │ Chaperoned │ Complete │  FF │
    ///  ╞═══════════╪════════════╪══════════╪═════╡
    ///  │    TRUE   │    FALSE   │     X    │ YES │
    ///  │    TRUE   │    TRUE    │   TRUE   │ YES │
    ///  │    TRUE   │    TRUE    │   FALSE  │  NO │
    ///  │   FALSE   │      X     │     X    │  NO │
    ///  └───────────┴────────────┴──────────┴─────┘
    /// ```
    ///
    /// Fast-forward means that no matter the transaction is committed or not,
    /// if there exist logs, discard them all without rolling back.
    ///
    /// States:
    ///  * **Committed**: Transaction is already committed but not complete
    ///               (Logs still exist).
    ///  * **Chaperoned**: The transaction was attached to a [`Chaperon::transaction`].
    ///  * **Complete**: The [`Chaperon::transaction`] is complete.
    ///
    /// [`Chaperon::transaction`]: ../chaperon/struct.Chaperon.html#method.transaction
    ///
    pub fn fast_forward(&self) -> bool {
        if !self.is_set(JOURNAL_COMMITTED) {
            false
        } else {
            if self.sec_id != 0 && !self.chaperon.is_empty() {
                let s = String::from_utf8(self.chaperon.to_vec()).unwrap();
                let c = unsafe { Chaperon::load(&s)
                    .expect(&format!("Missing chaperon file `{}`", s)) };
                if c.completed() {
                    true
                } else {
                    false
                }
            } else {
                true
            }
        }
    }

    pub(crate) fn start_session(&mut self, chaperon: &mut Chaperon) {
        let mut filename = [0u8; 64]; 
        let s = chaperon.filename().as_bytes();
        for i in 0..usize::min(64,s.len()) {
            filename[i] = s[i];
        }
        if self.sec_id != 0 {
            if self.chaperon != filename {
                panic!("Cannot attach to another chaperoned session");
            }
            return;
        }
        self.chaperon = filename;
        self.sec_id = chaperon.new_section() as u64;
    }

    pub(crate) fn complete(&mut self) {
        if self.sec_id != 0 && !self.chaperon.is_empty() {
            unsafe {
                let s = String::from_utf8(self.chaperon.to_vec()).unwrap();
                if let Ok(c) = Chaperon::load(&s) {
                    // If file not exists, it is on the normal path on the first
                    // execution. The existence of the file is already checked
                    // earlier in the recovery procedure.
                    let id = self.sec_id;
                    self.chaperon = [0; 64];
                    self.sec_id = 0;
                    persist_obj(&self.sec_id, true);
                    c.finish(id as usize);
                } else {
                    self.chaperon = [0; 64];
                    self.sec_id = 0;
                }
            }
        }
    }

    /// Returns the next journal for another transaction
    pub(crate) fn next(&self) -> Ptr<Journal<A>, A> {
        unsafe { Ptr::from_off_unchecked(self.next_off) }
    }

    /// Returns the offset of the next journal, if any. Otherwise, returns `u64::MAX`
    pub unsafe fn next_off(&self) -> u64 {
        self.next_off
    }

    /// Returns the offset of the previous journal, if any. Otherwise, returns `u64::MAX`
    pub unsafe fn prev_off(&self) -> u64 {
        self.prev_off
    }

    pub unsafe fn next_off_ref(&self) -> &u64 {
        &self.next_off
    }

    pub unsafe fn prev_off_ref(&self) -> &u64 {
        &self.prev_off
    }

    /// Returns a journal for the current thread. If there is no `Journal`
    /// object for the running thread, it may create a new journal and returns
    /// its mutable reference. Each thread may have only one journal.
    #[track_caller]
    pub(crate) fn current(create: bool) -> Option<(*const Journal<A>, *mut i32)>
    where
        Self: Sized,
    {
        unsafe {
            let tid = std::thread::current().id();
            A::journals(|journals| {
                if !journals.contains_key(&tid) && create {
                    #[cfg(feature = "stat_perf")]
                    let _perf = crate::stat::Measure::<A>::NewJournal(std::time::Instant::now());

                    let (journal, offset, _, z) = A::atomic_new(Journal::<A>::new(A::tx_gen()));
                    journal.enter_into(A::journals_head(), z);
                    A::perform(z);
                    journals.insert(tid, (offset, 0));
                }
                if let Some((j, c)) = journals.get_mut(&tid) {
                    Some((Ptr::<Self, A>::from_off_unchecked(*j).as_ptr(), c as *mut i32))
                } else {
                    None
                }
            })
        }
    }

    /// Returns true if there is a running transaction on the current thread
    pub fn is_running() -> bool {
        if let Some((_, cnt)) = Self::try_current() {
            unsafe {*cnt != 0}
        } else {
            false
        }
    }

    /// Returns a journal for the current thread. If there is no `Journal`
    /// object for the running thread, it may create a new journal and returns
    /// its mutable reference. Each thread may have only one journal.
    pub(crate) fn try_current() -> Option<(*const Journal<A>, *mut i32)>
    where
        Self: Sized,
    {
        unsafe {
            let tid = std::thread::current().id();
            A::journals(|journals| {
                if !journals.contains_key(&tid) {
                    None
                } else {
                    if let Some((j, c)) = journals.get_mut(&tid) {
                        Some((Ptr::<Self, A>::from_off_unchecked(*j).as_ptr(), c as *mut i32))
                    } else {
                        None
                    }
                }
            })
        }
    }

    /// Ignores all logs
    /// 
    /// This function is only for measuring some properties such as log latency.
    pub unsafe fn ignore(&self) {
        let mut page = utils::as_mut(self).pages.as_option();
        while let Some(p) = page {
            p.ignore();
            page = p.next.as_option();
        }
    }
}

impl<A: MemPool> Debug for Journal<A> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln!(f, "LOGS:")?;
        let mut curr = self.pages.clone();
        while let Some(page) = curr.as_option() {
            page.fmt(f)?;
            curr = page.next;
        }
        Ok(())
    }
}