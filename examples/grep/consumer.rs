use crate::hashmap::HashMap;
use crate::stack::Stack;
use crndm::default::*;
use crndm::sync::VWeak;
use regex::Regex;

type P = BuddyAlloc;

struct ConsumerData {
    buf: PString,
    local: HashMap<PString, u64>,
    active: bool,
}

pub struct Consumer {
    pattern: PString,
    data: PMutex<ConsumerData>,
    lines: Parc<PMutex<Stack<PString>>>,
    words: Parc<PMutex<HashMap<PString, u64>>>,
}

impl Consumer {
    pub fn new(
        pattern: &str,
        lines: Parc<PMutex<Stack<PString>>>,
        words: Parc<PMutex<HashMap<PString, u64>>>,
        j: &Journal,
    ) -> Self {
        Self {
            pattern: PString::from_str(pattern, j),
            lines,
            words,
            data: PMutex::new(
                ConsumerData {
                    buf: PString::new(j),
                    local: HashMap::new(j),
                    active: true,
                },
                j,
            ),
        }
    }

    /// Starts processing `lines` and updating `words`
    pub fn start(slf: VWeak<Consumer, P>) {
        loop {
            // Read from global buffer to the local buffer
            if !P::transaction(|j| {
                if let Some(slf) = slf.upgrade(j) {
                    let mut this = slf.data.lock(j);
                    if this.buf.is_empty() {
                        let mut lines = slf.lines.lock(j);
                        let line = lines.pop(j);
                        eprint!(
                            "\r\x1b[?25lRemaining: {:<12} Memory usage: {:<9} bytes \x1b[?25h",
                            lines.len(),
                            P::used()
                        );
                        if let Some(line) = line {
                            this.buf = line;
                            true // Still working
                        } else {
                            this.active
                        }
                    } else {
                        true
                    }
                } else {
                    false
                }
            })
            .unwrap()
            {
                return;
            }

            // counting words
            P::transaction(|j| {
                if let Some(slf) = slf.upgrade(j) {
                    let mut this = slf.data.lock(j);
                    if !this.buf.is_empty() {
                        let buf = this.buf.to_string();
                        let re = Regex::new(slf.pattern.as_str()).unwrap();

                        for cap in re.captures_iter(&buf) {
                            let w = cap.get(1).unwrap().as_str().to_pstring(j);
                            this.local.update_with(&w, j, |v| v + 1);
                        }
                        this.buf.clear();
                    }
                }
            })
            .unwrap();

            // Updating global `words` buffer with the local buffer
            P::transaction(|j| {
                if let Some(slf) = slf.upgrade(j) {
                    let mut this = slf.data.lock(j);
                    let mut words = slf.words.lock(j);
                    this.local.foreach(|k, v| {
                        words.update_with(k, j, |v0| v0 + v);
                    });
                    this.local.clear(j);
                }
            })
            .unwrap();
        }
    }

    pub fn stop_when_finished(&self) {
        P::transaction(|j| {
            let mut this = self.data.lock(j);
            this.active = false;
        })
        .unwrap();
    }
}
