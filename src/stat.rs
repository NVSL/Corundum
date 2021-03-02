#![cfg(target_arch = "x86_64")]

use std::any::type_name;
use std::any::Any;
use std::marker::PhantomData;
use std::collections::HashMap;
use std::fmt::Display;
use std::ops::AddAssign;
use std::sync::Mutex;
use std::thread::{current, ThreadId};
use std::time::Instant;
use std::io::*;
use crate::cell::LazyCell;

#[derive(Clone)]
struct Data {
    sum: u64,
    cnt: u64,
    sum2: f64,
    min: u64,
    max: u64, 
    points: HashMap<u64, u64>
}

impl Default for Data {
    fn default() -> Self {
        Data { sum: 0, cnt: 0, sum2: 0f64, min: u64::MAX, max:0,
            points: Default::default()
        }
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
    Sync(Instant),
    Alloc(Instant),
    Dealloc(Instant),
    Deref(Instant),
    DropLog(Instant),
    DataLog(Instant),
    MutexLog(Instant),
    CommitLog(Instant),
    RollbackLog(Instant),
    ClearLog(Instant),
    NewPage(Instant),
    NewJournal(Instant),
    Logging(Instant),
    Nop(Instant),
    Custom(Instant, String),
    Batch(Instant, String, u64),
    Transaction,
    Unknown(PhantomData<A>)
}

static mut HIST: Option<bool> = None;
static mut POINTS: Option<bool> = None;

static mut STAT: LazyCell<Mutex<HashMap<(ThreadId, &'static str), Stat>>> = 
    LazyCell::new(|| Mutex::new(HashMap::new()));

#[inline]
fn hist_enabled() -> bool {
    unsafe {
        if let Some(hist) = &mut HIST {
            *hist
        } else {
            if let Some(val) = std::env::var_os("HIST") {
                HIST = Some(val.into_string().unwrap().parse::<i32>().unwrap() == 1);
                true
            } else {
                HIST = Some(false);
                false
            }
        }
    }
}

#[inline]
fn points_enabled() -> bool {
    unsafe {
        if let Some(points) = &mut POINTS {
            *points
        } else {
            if let Some(val) = std::env::var_os("POINTS") {
                POINTS = Some(val.into_string().unwrap().parse::<i32>().unwrap() == 1);
                true
            } else {
                POINTS = Some(false);
                false
            }
        }
    }
}

macro_rules! add {
    ($tp:ty,$s:ident,custom,$m:expr) => {
        // let mut t = tsc();
        // t -= *$s;
        let t = $s.elapsed().as_nanos() as u64;
        let mut stat = match unsafe { STAT.lock() } {
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
        if hist_enabled() {
            let p = counter.points.entry(t).or_default();
            *p += 1;
        }
    };
    ($tp:ty,$s:ident,batch,$m:expr,$cnt:expr) => {
        // let mut t = tsc();
        // t -= *$s;
        let t = $s.elapsed().as_nanos() as u64;
        let mut stat = match unsafe { STAT.lock() } {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        let tid = current().id();
        let stat = stat.entry((tid,type_name::<$tp>())).or_default();
        let counter = stat.custom.entry($m).or_default();
        counter.sum += t;
        counter.cnt += $cnt;
        counter.sum2 = 0f64;
        counter.min = 0;
        counter.max = 0;
        if hist_enabled() {
            let p = counter.points.entry(t).or_default();
            *p += 1;
        }
    };
    ($tp:ty,$s:ident,$id:ident,$cnt:ident) => {
        // let mut t = tsc();
        // t -= *$s;
        let t = $s.elapsed().as_nanos() as u64;
        let mut stat = match unsafe { STAT.lock() } {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        let tid = current().id();
        let stat = stat.entry((tid,type_name::<$tp>())).or_default();
        stat.$id += t;
        stat.$cnt += 1;
    };
    ($tp:ty,$s:ident,$id:ident) => {
        // let mut t = tsc();
        // t -= *$s;
        let t = $s.elapsed().as_nanos() as u64;
        let mut stat = match unsafe { STAT.lock() } {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        let tid = current().id();
        let stat = stat.entry((tid,type_name::<$tp>())).or_default();
        stat.$id += t;
    };
    ($tp:ty,$cnt:ident) => {
        let mut stat = match unsafe { STAT.lock() } {
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
            if hist_enabled() {
                for (vp,vv) in &v.points {
                    let p = counter.points.entry(*vp).or_default();
                    *p += vv;
                }
            }
        }
    }
}

impl Stat {
    pub fn save_histograms(&self, _path: &str) -> Result<()> {
        if hist_enabled() {
            for (k,v) in &self.custom {
                use std::fs::File;
                use prelude::*;

                let mut f = File::create(format!("{}/{}_hist.csv", _path, k))?;
                f.write(b"lat,freq\n")?;

                let mut pairs = vec![];
                for (tm,fr) in &v.points {
                    pairs.push((tm,fr));
                }
                pairs.sort_by(|x, y| x.cmp(&y));

                for (tm,fr) in &pairs {
                    f.write(format!("{},{}\n", tm, fr).to_string().as_bytes())?;
                }

                if points_enabled() {
                    let mut f = File::create(format!("{}/{}_points.csv", _path, k))?;
                    f.write(format!("{}\n", k).to_string().as_bytes())?;
                    for (tm,fr) in &pairs {
                        for _ in 0..**fr {
                            f.write(format!("{}\n", tm).to_string().as_bytes())?;
                        }
                    }
                }
            }
        }
        Ok(())
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
        #[cfg(feature = "stat_perf")] {
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

        for (k,v) in &self.custom {
            let avg = div(v.sum, v.cnt);
            let sd = f64::sqrt(v.sum2/(v.cnt as f64)-f64::powi(avg,2));
            lns.push(format!("{:<15}{:>10} ns  avg(ns): {:<11.3} std(ns): {:<8.1} min(ns): {:<8} max(ns): {:<10} cnt: {}",
                k, v.sum, avg, sd,
                v.min, v.max, v.cnt));
        }
        
        lns.sort_by(|x, y| x.cmp(&y));
        for ln in &lns {
            writeln!(f, "{}", ln)?;
        }

        if hist_enabled() {
            let mut _plots = vec!();
            for (k,v) in &self.custom {
                if let Some((plt,min,max,vmax,avg)) = plot(&v.points, 1.0, 10) {
                    let mut plot = format!("┌{:─^80}┐{}\n", format!(" {} ", k), vmax);
                    for ln in plt {
                        plot += &format!("│{}│\n", ln);
                    }
                    plot += &format!("└{:─^80}┘\n", "┼");
                    plot += &format!("{:<31}{: ^20}{:>31}", min, avg, max);
                    _plots.push(plot);
                }
            }
            
            _plots.sort_by(|x, y| x.replace('─',"").cmp(&y.replace('─',"")));
            for pl in &_plots {
                writeln!(f, "{}", pl)?;
            }
        }

        Ok(())
    }
}

pub fn report() -> String {
    let stat = match unsafe { STAT.lock() } {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    let mut total = Stat::default();
    let mut res = String::new();
    let print_all_threads = stat.len() > 1;
    for (tid, stat) in stat.iter() {
        if print_all_threads {
            res += &format!(
                "\n{:-^113}\n{}",
                format!(" Performance Details {:?} ", tid), stat
            );
        }
        total += stat;
    }
    format!(
        "{}\n{:=^113}\n{}",
        res, " All Threads and Pool Types ", total
    )
}

pub fn save_histograms(_path: &'static str) -> Result<()> {
    if hist_enabled() {
        let stat = match unsafe { STAT.lock() } {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        let mut total = Stat::default();
        for (_, stat) in stat.iter() {
            total += stat;
        }
        total.save_histograms(_path)
    } else {
        println!("Use HIST=1 environment variable to enable histogram info.");
        Err(Error::from(ErrorKind::Other))
    }
}

fn plot(data: &HashMap<u64, u64>, x: f32, freq_thr: u64) -> Option<(Vec<String>,i64,i64,i64,i64)> {
    let mut res = vec!["                                                                                ".to_string(); 20];
    let mut freqs = vec![0; 80];
    let h_min = data.keys().min()?;
    let h_max = data.keys().max()?;
    let h_len = h_max - h_min;
    if h_len == 0 {
        None
    } else {
        let mut sum = 0;
        let mut cnt = 0;
        for (t,freq) in data {
            if *freq > freq_thr {
                sum += freq * *t;
                cnt += freq;
            }
        }
        if cnt > 0 {
            let avg = (sum / cnt) as i64;
    
            for (t,freq) in data {
                let t = (*t as i64) - avg;
    
                let t = (x * (t as f32 * 40.0) / avg as f32) as i64;
                let t = 0.max(79.min(t + 40));
                freqs[t as usize] += freq;
            }
    
            let v_max = freqs.iter().max()?;
            for i in 0..freqs.len() {
                let f = (freqs[i] * 19) / v_max;
                let f = 19.min(f as usize);
                for j in 0..f {
                    unsafe { res[19-j].as_bytes_mut()[i] = b'X'; }
                }
            }
            Some((res,
                ((1.0 - x) * avg as f32) as i64,
                ((1.0 + x) * avg as f32) as i64,
                *v_max as i64,
                avg))
        } else {
            None
        }
    }
}

#[macro_export]
macro_rules! measure {
    ($tag:expr,$n:expr,$f:block) => {
        {
            let __tag = $tag;
            {
                #[allow(unused_import)]
                use std::time::Instant;

                let mut _perf = Measure::<P>::Batch(Instant::now(), __tag, $n as u64);
                let mut _dummy = Instant::now();
                let mut _rt = &mut _dummy;
                if let Measure::<P>::Batch(t, _, _) = &mut _perf {
                    _rt = t;
                }
                *_rt = Instant::now();
                $f
            }
        }
    };
    ($tag:expr,$f:block) => {
        {
            let __tag = $tag;
            {
                #[allow(unused_import)]
                use std::time::Instant;

                let mut _perf = Measure::<P>::Custom(Instant::now(), __tag);
                let mut _dummy = Instant::now();
                let mut _rt = &mut _dummy;
                if let Measure::<P>::Custom(t, _) = &mut _perf {
                    _rt = t;
                }
                *_rt = Instant::now();
                $f
            }
        }
    };
}