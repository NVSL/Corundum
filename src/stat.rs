use std::time::Instant;

static mut SYNC:          u128 = 0;
static mut ALLOC:         u128 = 0;
static mut DEALLOC:       u128 = 0;
static mut DROP_LOG:      u128 = 0;
static mut DATA_LOG:      u128 = 0;
static mut MUTEX_LOG:     u128 = 0;
static mut COMMIT:        u128 = 0;
static mut ROLLBACK:      u128 = 0;
static mut CLEAR:         u128 = 0;
static mut NEW_PAGE:      u128 = 0;
static mut NEW_JRNL:      u128 = 0;
static mut LOGGING:       u128 = 0;

static mut CNT_SYNC:      u64 = 0;
static mut CNT_ALLOC:     u64 = 0;
static mut CNT_DEALLOC:   u64 = 0;
static mut CNT_DROP_LOG:  u64 = 0;
static mut CNT_DATA_LOG:  u64 = 0;
static mut CNT_MUTEX_LOG: u64 = 0;
static mut CNT_COMMIT:    u64 = 0;
static mut CNT_ROLLBACK:  u64 = 0;
static mut CNT_CLEAR:     u64 = 0;
static mut CNT_NEW_PAGE:  u64 = 0;
static mut CNT_NEW_JRNL:  u64 = 0;

pub enum Measure {
    Sync(Instant),
    Alloc(Instant),
    Dealloc(Instant),
    DropLog(Instant),
    DataLog(Instant),
    MutexLog(Instant),
    CommitLog(Instant),
    RollbackLog(Instant),
    ClearLog(Instant),
    NewPage(Instant),
    NewJournal(Instant),
    Logging(Instant)
}

use Measure::*;

impl Drop for Measure {
    #[inline]
    fn drop(&mut self) {
        unsafe {
            match self {
                Sync(s)       => { SYNC      += s.elapsed().as_micros(); CNT_SYNC       += 1; }
                Alloc(s)       => { ALLOC     += s.elapsed().as_micros(); CNT_ALLOC     += 1; }
                Dealloc(s)     => { DEALLOC   += s.elapsed().as_micros(); CNT_DEALLOC   += 1; }
                DropLog(s)     => { DROP_LOG  += s.elapsed().as_micros(); CNT_DROP_LOG  += 1; }
                DataLog(s)     => { DATA_LOG  += s.elapsed().as_micros(); CNT_DATA_LOG  += 1; }
                MutexLog(s)    => { MUTEX_LOG += s.elapsed().as_micros(); CNT_MUTEX_LOG += 1; }
                CommitLog(s)   => { COMMIT    += s.elapsed().as_micros(); CNT_COMMIT    += 1; }
                RollbackLog(s) => { ROLLBACK  += s.elapsed().as_micros(); CNT_ROLLBACK  += 1; }
                ClearLog(s)    => { CLEAR     += s.elapsed().as_micros(); CNT_CLEAR     += 1; }
                NewPage(s)     => { NEW_PAGE  += s.elapsed().as_micros(); CNT_NEW_PAGE  += 1; }
                NewJournal(s)  => { NEW_JRNL  += s.elapsed().as_micros(); CNT_NEW_JRNL  += 1; }
                Logging(s)     => { LOGGING   += s.elapsed().as_micros(); }
            }
        }
    }
}

pub fn report() -> String {
unsafe { format!(
"Performance Details
===================
Sync        {:>10} us   {}
Alloc       {:>10} us   {}
Dealloc     {:>10} us   {}
DropLog     {:>10} us   {}
DataLog     {:>10} us   {}
MutexLog    {:>10} us   {}
Commit      {:>10} us   {}
Rollback    {:>10} us   {}
Del Log     {:>10} us   {}
New Page    {:>10} us   {}
New Journal {:>10} us   {}
Logging     {:>10} us",
SYNC,      CNT_SYNC,  
ALLOC,     CNT_ALLOC,  
DEALLOC,   CNT_DEALLOC, 
DROP_LOG,  CNT_DROP_LOG,
DATA_LOG,  CNT_DATA_LOG,
MUTEX_LOG, CNT_MUTEX_LOG,
COMMIT,    CNT_COMMIT,
ROLLBACK,  CNT_ROLLBACK,
CLEAR,     CNT_CLEAR,
NEW_PAGE,  CNT_NEW_PAGE,
NEW_JRNL,  CNT_NEW_JRNL,
LOGGING ) }
}