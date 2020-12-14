use crndm::default::*;
use std::fmt::Display;

type P = BuddyAlloc;

struct StackItem<T: PSafe> {
    data: T,
    next: Option<Parc<StackItem<T>>>,
}

pub struct Stack<T: PSafe> {
    len: usize,
    head: Option<Parc<StackItem<T>>>,
}

impl<T: PSafe> Stack<T> {
    pub fn new() -> Self {
        Self { len: 0, head: None }
    }

    pub fn push(&mut self, data: T, j: &Journal) {
        self.head = Some(Parc::new(
            StackItem {
                data,
                next: self.head.pclone(j),
            },
            j,
        ));
        self.len += 1;
    }

    pub fn pop(&mut self, j: &Journal) -> Option<T>
    where
        T: PClone<P>,
    {
        if let Some(head) = &self.head {
            let d = head.data.pclone(j);
            self.head = head.next.pclone(j);
            self.len -= 1;
            Some(d)
        } else {
            None
        }
    }

    pub fn clear(&mut self) {
        self.head = None;
        self.len = 0;
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn print_top10(&self)
    where
        T: Display,
    {
        let mut curr = &self.head;
        for i in 0..10 {
            if let Some(c) = curr {
                print!("{:2>}: {}", i + 1, c.data);
                curr = &c.next;
            } else {
                break;
            }
        }
        println!(
            "----------------------------------------------------- Total: {}",
            self.len
        );
    }

    pub fn print_all(&self)
    where
        T: Display,
    {
        let mut curr = &self.head;
        for i in 0..self.len() {
            if let Some(c) = curr {
                print!("{:2>}: {}", i + 1, c.data);
                curr = &c.next;
            } else {
                break;
            }
        }
        println!(
            "----------------------------------------------------- Total: {}",
            self.len
        );
    }
}
