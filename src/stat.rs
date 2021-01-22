#![cfg(target_arch = "x86_64")]

use std::any::type_name;
use std::any::Any;
use std::marker::PhantomData;
use std::collections::HashMap;
use std::fmt::Display;
use std::ops::AddAssign;
use std::sync::Mutex;
use std::thread::{current, ThreadId};

static mut FREQ: f64 = 1.0f64;

#[derive(Clone)]
struct Data {
    sum: u64,
    cnt: u64,
    sum2: f64,
    min: u64,
    max: u64,

    #[cfg(features="plot_histogram")] 
    points: HashMap<u64, u64>
}

impl Default for Data {
    fn default() -> Self {
        Data { sum: 0, cnt: 0, sum2: 0f64, min: u64::MAX, max:0 }
    }
}

#[inline(always)]
pub fn tsc() -> u64 {
    unsafe {
        // Flush the pipeline
        // llvm_asm!("xor %eax, %eax
        // cpuid");
        core::arch::x86_64::_rdtsc()
    }
}

#[derive(Default, Clone)]
struct Stat {
    sync: u64,
    cnt_sync: u64,
    alloc: u64,
    cnt_alloc: u64,
    dealloc: u64,
    cnt_dealloc: u64,
    deref: u64,
    cnt_deref: u64,
    drop_log: u64,
    cnt_drop_log: u64,
    data_log: u64,
    cnt_data_log: u64,
    mutex_log: u64,
    cnt_mutex_log: u64,
    commit: u64,
    cnt_commit: u64,
    rollback: u64,
    cnt_rollback: u64,
    clear: u64,
    cnt_clear: u64,
    new_page: u64,
    cnt_new_page: u64,
    new_jrnl: u64,
    cnt_new_jrnl: u64,
    logging: u64,
    cnt_logging: u64,
    nop: u64,
    cnt_nop: u64,
    custom: HashMap<String, Data>
}

pub enum Measure<A: Any> {
    Sync(u64),
    Alloc(u64),
    Dealloc(u64),
    Deref(u64),
    DropLog(u64),
    DataLog(u64),
    MutexLog(u64),
    CommitLog(u64),
    RollbackLog(u64),
    ClearLog(u64),
    NewPage(u64),
    NewJournal(u64),
    Logging(u64),
    Nop(u64),
    Custom(u64, String),
    Batch(u64, String, u64),
    Transaction,
    Unknown(PhantomData<A>)
}

lazy_static! {
    static ref STAT: Mutex<HashMap<(ThreadId, &'static str), Stat>> = Mutex::new(HashMap::new());
}

macro_rules! add {
    ($tp:ty,$s:ident,custom,$m:expr) => {
        let mut t = tsc();
        t -= *$s;
        let mut stat = match STAT.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        let tid = current().id();
        let stat = stat.entry((tid,type_name::<$tp>())).or_default();
        let counter = stat.custom.entry($m).or_default();
        counter.sum += t;
        counter.cnt += 1;
        counter.sum2 += f64::powi(t as f64, 2);
        if counter.max < t { counter.max = t; }
        if counter.min > t { counter.min = t; }
        // let p = counter.points.entry(t/10).or_default();
        // *p += 1;
    };
    ($tp:ty,$s:ident,batch,$m:expr,$cnt:expr) => {
        let mut t = tsc();
        t -= *$s;
        let mut stat = match STAT.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        let tid = current().id();
        let stat = stat.entry((tid,type_name::<$tp>())).or_default();
        let counter = stat.custom.entry($m).or_default();
        let avg = t as f64/$cnt as f64;
        counter.sum += t;
        counter.cnt += $cnt;
        counter.sum2 += f64::powi(avg, 2) * $cnt as f64;
        let min = f64::trunc(avg) as u64;
        let max = f64::ceil(avg) as u64;
        if counter.max < max { counter.max = max; }
        if counter.min > min { counter.min = min; }
    };
    ($tp:ty,$s:ident,$id:ident,$cnt:ident) => {
        let mut t = tsc();
        t -= *$s;
        let mut stat = match STAT.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        let tid = current().id();
        let stat = stat.entry((tid,type_name::<$tp>())).or_default();
        stat.$id += t;
        stat.$cnt += 1;
    };
    ($tp:ty,$s:ident,$id:ident) => {
        let mut t = tsc();
        t -= *$s;
        let mut stat = match STAT.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        let tid = current().id();
        let stat = stat.entry((tid,type_name::<$tp>())).or_default();
        stat.$id += t;
    };
    ($tp:ty,$cnt:ident) => {
        let mut stat = match STAT.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        let tid = current().id();
        let stat = stat.entry((tid,type_name::<$tp>())).or_default();
        stat.$cnt += 1;
    };
}

use Measure::*;

impl<A: Any> Drop for Measure<A> {
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
            Deref(s) => {
                add!(A, s, deref, cnt_deref);
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
            Nop(s) => {
                add!(A, s, nop, cnt_nop);
            }
            Custom(s, m) => {
                add!(A, s, custom, m.to_string());
            }
            Batch(s, m, cnt) => {
                add!(A, s, batch, m.to_string(), *cnt);
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
        self.deref += d.deref;
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
        self.cnt_deref += d.cnt_deref;
        self.cnt_drop_log += d.cnt_drop_log;
        self.cnt_data_log += d.cnt_data_log;
        self.cnt_mutex_log += d.cnt_mutex_log;
        self.cnt_commit += d.cnt_commit;
        self.cnt_rollback += d.cnt_rollback;
        self.cnt_clear += d.cnt_clear;
        self.cnt_new_page += d.cnt_new_page;
        self.cnt_new_jrnl += d.cnt_new_jrnl;
        for (k,v) in &d.custom {
            let counter = self.custom.entry(k.to_string()).or_default();
            counter.cnt += v.cnt;
            counter.sum += v.sum;
            counter.sum2 += v.sum2;
            if counter.max < v.max { counter.max = v.max; }
            if counter.min > v.min { counter.min = v.min; }
            #[cfg(features="plot_histogram")] {
                for (vp,vv) in &v.points {
                    let p = counter.points.entry(*vp).or_default();
                    *p += vv;
                }
            }
        }
    }
}

fn div(a: u64, b: u64) -> f64 {
    if b == 0 {
        0f64
    } else {
        a as f64 / b as f64
    }
}

impl Display for Stat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        #[cfg(feature = "perf_stat")] {
            writeln!(
                f,
"Sync          {:>14} us    avg(ns): {:<8}    cnt: {}
Alloc         {:>14} ns    avg(ns): {:<8}    cnt: {}
Dealloc       {:>14} ns    avg(ns): {:<8}    cnt: {}
AdrTrans      {:>14} ns    avg(ns): {:<8}    cnt: {}
DropLog       {:>14} ns    avg(ns): {:<8}    cnt: {}
DataLog       {:>14} ns    avg(ns): {:<8}    cnt: {}
MutexLog      {:>14} ns    avg(ns): {:<8}    cnt: {}
Commit        {:>14} ns    avg(ns): {:<8}    cnt: {}
Rollback      {:>14} ns    avg(ns): {:<8}    cnt: {}
Del Log       {:>14} ns    avg(ns): {:<8}    cnt: {}
New Page      {:>14} ns    avg(ns): {:<8}    cnt: {}
New Journal   {:>14} ns    avg(ns): {:<8}    cnt: {}
Logging       {:>14} ns    avg(ns): {:<8}    cnt: {}",
                self.sync,
                div(self.sync, self.cnt_sync),
                self.cnt_sync,
                self.alloc,
                div(self.alloc, self.cnt_alloc),
                self.cnt_alloc,
                self.dealloc,
                div(self.dealloc, self.cnt_dealloc),
                self.cnt_dealloc,
                self.deref,
                div(self.deref, self.cnt_deref),
                self.cnt_deref,
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
            )?;
        }
        let mut lns = vec!();

        #[cfg(features="plot_histogram")] {
            let mut plots = String::new();
        }

        for (k,v) in &self.custom {
            let avg = div(v.sum, v.cnt);
            let sd = f64::sqrt(v.sum2/(v.cnt as f64)-f64::powi(avg,2));
            unsafe {
                lns.push(format!("{:<15}{:>10.0} ns  avg(ns): {:<11.3} std(ns): {:<8.1} min(ns): {:<8.0} max(ns): {:<10.0} cnt: {}",
                    k,
                    v.sum as f64 / FREQ,
                    avg as f64 / FREQ,
                    sd as f64 / FREQ,
                    v.min as f64 / FREQ,
                    v.max as f64 / FREQ,
                    v.cnt));
            }
            #[cfg(features="plot_histogram")] {
                if let Some(plt) = plot(&v.points) {
                    plots += &format!("┌{:─^40}┐\n", format!(" {} ", k));
                    for ln in plt {
                        plots += &format!("│{}│\n", ln);
                    }
                    plots += "└────────────────────────────────────────┘\n";
                }
            }
        }
        
        lns.sort_by(|x, y| x.cmp(&y));
        for ln in &lns {
            writeln!(f, "{}", ln)?;
        }

        #[cfg(features="plot_histogram")] {
            writeln!(f, "{}", plots)?;
        }
        Ok(())
    }
}

pub fn report() -> String {
    let stat = match STAT.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    let mut total = Stat::default();
    let mut res = String::new();
    let print_all_threads = stat.len() > 1;
    unsafe {
        FREQ = executor::block_on(heim_cpu::frequency()).unwrap().current().get::<uom::si::frequency::hertz>() as f64;
        FREQ *= 1e-9;
        for (tid, stat) in stat.iter() {
            if print_all_threads {
                res += &format!(
                    "\n{:-^113}\n{}",
                    format!("Performance Details {:?} ({:.1} GHz)", tid, FREQ), stat
                );
            }
            total += stat;
        }
        format!(
            "{}\n{:=^113}\n{}",
            res, format!(" All Threads and Pool Types ({:.1} GHz) ", FREQ), total
        )
    }
}


#[cfg(features="plot_histogram")]
fn plot(data: &HashMap<u64, u64>) -> Option<Vec<String>> {
    let mut res = vec!["                                        ".to_string(); 20];
    let mut freqs = vec![0; 40];
    let h_min = data.keys().min()?;
    let h_max = data.keys().max()?;
    let h_len = h_max - h_min;
    for (t,freq) in data {
        let t = ((t - h_min) * 390) / h_len;
        let t = 39.min(t as usize);
        freqs[t as usize] += freq;
    }
    let v_max = freqs.iter().max()?;
    for i in 0..freqs.len() {
        let f = (freqs[i] * 19) / v_max;
        let f = 19.min(f as usize);
        for j in 0..f {
            unsafe { res[19-j].as_bytes_mut()[i] = b'A'; }
        }
    }
    Some(res)
}

#[macro_export]
macro_rules! measure {
    ($tag:expr,$n:expr,$f:block) => {
        {
            let __tag = $tag;
            {
                let mut _perf = Measure::<P>::Batch(0, __tag, $n as u64);
                let mut _dummy: u64 = 0;
                let mut _rt = &mut _dummy;
                if let Measure::<P>::Batch(t, _, _) = &mut _perf {
                    _rt = t;
                }
                *_rt = $crate::stat::tsc();
                $f
            }
        }
    };
    ($tag:expr,$f:block) => {
        {
            let __tag = $tag;
            {
                let mut _perf = Measure::<P>::Custom(0, __tag);
                let mut _dummy: u64 = 0;
                let mut _rt = &mut _dummy;
                if let Measure::<P>::Custom(t, _) = &mut _perf {
                    _rt = t;
                }
                *_rt = $crate::stat::tsc();
                $f
            }
        }
    };
}