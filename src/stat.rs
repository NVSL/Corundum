use std::any::type_name;
use std::any::Any;
use std::marker::PhantomData;
use crate::alloc::MemPool;
use std::collections::HashMap;
use std::fmt::Display;
use std::ops::AddAssign;
use std::sync::Mutex;
use std::thread::{current, ThreadId};
use std::time::Instant;

#[derive(Default, Copy, Clone)]
struct Stat {
    sync: u128,
    cnt_sync: u128,
    alloc: u128,
    cnt_alloc: u128,
    dealloc: u128,
    cnt_dealloc: u128,
    drop_log: u128,
    cnt_drop_log: u128,
    data_log: u128,
    cnt_data_log: u128,
    mutex_log: u128,
    cnt_mutex_log: u128,
    commit: u128,
    cnt_commit: u128,
    rollback: u128,
    cnt_rollback: u128,
    clear: u128,
    cnt_clear: u128,
    new_page: u128,
    cnt_new_page: u128,
    new_jrnl: u128,
    cnt_new_jrnl: u128,
    logging: u128,
    cnt_logging: u128,
}

pub enum Measure<A:MemPool+Any> {
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
    Logging(Instant),
    Transaction,
    Unknown(PhantomData<A>)
}

lazy_static! {
    static ref STAT: Mutex<HashMap<(ThreadId, &'static str), Stat>> = Mutex::new(HashMap::new());
}

macro_rules! add {
    ($tp:ty,$s:ident,$id:ident,$cnt:ident) => {
        let t = $s.elapsed().as_micros();
        let mut stat = match STAT.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        let tid = current().id();
        let stat = stat.entry((tid,type_name::<$tp>())).or_insert(Default::default());
        stat.$id += t;
        stat.$cnt += 1;
    };
    ($tp:ty,$s:ident,$id:ident) => {
        let t = $s.elapsed().as_micros();
        let mut stat = match STAT.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        let tid = current().id();
        let stat = stat.entry((tid,type_name::<$tp>())).or_insert(Default::default());
        stat.$id += t;
    };
    ($tp:ty,$cnt:ident) => {
        let mut stat = match STAT.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        let tid = current().id();
        let stat = stat.entry((tid,type_name::<$tp>())).or_insert(Default::default());
        stat.$cnt += 1;
    };
}

use Measure::*;

impl<A: MemPool + Any> Drop for Measure<A> {
    #[inline]
    fn drop(&mut self) {
        match self {
            Sync(s) => {
                add!(A, s, sync, cnt_sync);
            }
            Alloc(s) => {
                add!(A, s, alloc, cnt_alloc);
            }
            Dealloc(s) => {
                add!(A, s, dealloc, cnt_dealloc);
            }
            DropLog(s) => {
                add!(A, s, drop_log, cnt_drop_log);
            }
            DataLog(s) => {
                add!(A, s, data_log, cnt_data_log);
            }
            MutexLog(s) => {
                add!(A, s, mutex_log, cnt_mutex_log);
            }
            CommitLog(s) => {
                add!(A, s, commit, cnt_commit);
            }
            RollbackLog(s) => {
                add!(A, s, rollback, cnt_rollback);
            }
            ClearLog(s) => {
                add!(A, s, clear, cnt_clear);
            }
            NewPage(s) => {
                add!(A, s, new_page, cnt_new_page);
            }
            NewJournal(s) => {
                add!(A, s, new_jrnl, cnt_new_jrnl);
            }
            Logging(s) => {
                add!(A, s, logging);
            }
            Transaction => {
                add!(A, cnt_logging);
            }
            _ => {}
        }
    }
}

impl AddAssign<&Stat> for Stat {
    fn add_assign(&mut self, d: &Stat) {
        self.sync += d.sync;
        self.alloc += d.alloc;
        self.dealloc += d.dealloc;
        self.drop_log += d.drop_log;
        self.data_log += d.data_log;
        self.mutex_log += d.mutex_log;
        self.commit += d.commit;
        self.rollback += d.rollback;
        self.clear += d.clear;
        self.new_page += d.new_page;
        self.new_jrnl += d.new_jrnl;
        self.logging += d.logging;
        self.cnt_logging += d.cnt_logging;
        self.cnt_sync += d.cnt_sync;
        self.cnt_alloc += d.cnt_alloc;
        self.cnt_dealloc += d.cnt_dealloc;
        self.cnt_drop_log += d.cnt_drop_log;
        self.cnt_data_log += d.cnt_data_log;
        self.cnt_mutex_log += d.cnt_mutex_log;
        self.cnt_commit += d.cnt_commit;
        self.cnt_rollback += d.cnt_rollback;
        self.cnt_clear += d.cnt_clear;
        self.cnt_new_page += d.cnt_new_page;
        self.cnt_new_jrnl += d.cnt_new_jrnl;
    }
}

fn div(a: u128, b: u128) -> u128 {
    if b == 0 {
        0
    } else {
        (1000 * a) / b
    }
}

impl Display for Stat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        writeln!(
            f,
            "Sync        {:>10} us    avg(ns): {:<10}    cnt: {}
Alloc       {:>10} us    avg(ns): {:<10}    cnt: {}
Dealloc     {:>10} us    avg(ns): {:<10}    cnt: {}
DropLog     {:>10} us    avg(ns): {:<10}    cnt: {}
DataLog     {:>10} us    avg(ns): {:<10}    cnt: {}
MutexLog    {:>10} us    avg(ns): {:<10}    cnt: {}
Commit      {:>10} us    avg(ns): {:<10}    cnt: {}
Rollback    {:>10} us    avg(ns): {:<10}    cnt: {}
Del Log     {:>10} us    avg(ns): {:<10}    cnt: {}
New Page    {:>10} us    avg(ns): {:<10}    cnt: {}
New Journal {:>10} us    avg(ns): {:<10}    cnt: {}
Logging     {:>10} us    avg(ns): {:<10}    cnt: {}",
            self.sync,
            div(self.sync, self.cnt_sync),
            self.cnt_sync,
            self.alloc,
            div(self.alloc, self.cnt_alloc),
            self.cnt_alloc,
            self.dealloc,
            div(self.dealloc, self.cnt_dealloc),
            self.cnt_dealloc,
            self.drop_log,
            div(self.drop_log, self.cnt_drop_log),
            self.cnt_drop_log,
            self.data_log,
            div(self.data_log, self.cnt_data_log),
            self.cnt_data_log,
            self.mutex_log,
            div(self.mutex_log, self.cnt_mutex_log),
            self.cnt_mutex_log,
            self.commit,
            div(self.commit, self.cnt_commit),
            self.cnt_commit,
            self.rollback,
            div(self.rollback, self.cnt_rollback),
            self.cnt_rollback,
            self.clear,
            div(self.clear, self.cnt_clear),
            self.cnt_clear,
            self.new_page,
            div(self.new_page, self.cnt_new_page),
            self.cnt_new_page,
            self.new_jrnl,
            div(self.new_jrnl, self.cnt_new_jrnl),
            self.cnt_new_jrnl,
            self.logging,
            div(self.logging, self.cnt_logging),
            self.cnt_logging
        )
    }
}

pub fn report() -> String {
    let stat = match STAT.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    let mut total = Stat::default();
    let mut res = String::new();
    for (tid, stat) in stat.iter() {
        res += &format!(
            "
Performance Details {:?}
-------------------------------------------------------------------
{}",
            tid, stat
        );
        total += stat;
    }
    format!(
        "{}
All Threads and Pool Types
===================================================================
{}",
        res, total
    )
}
