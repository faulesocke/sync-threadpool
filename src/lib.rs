// Copyright 2018 Urs Schulz
//
// This file is part of sync-threadpool.
//
// sync-threadpool is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// sync-threadpool is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with sync-threadpool.  If not, see <http://www.gnu.org/licenses/>.

//! A synchronized threadpool. This threadpool allows submitting new jobs only if a worker is
//! currently idle. This is most useful, for search-like tasks: Imagine you need to find some value
//! in a multithreaded manner. Once the value is found, of course you don't want to submit more
//! search jobs but probably want to use the workers for something else.
//!
//! # Examples
//! This example demonstrates finding square-roots by brute forcing. The roots are found by sending
//! search-ranges to the threadpool.
//! ```
//! use sync_threadpool::ThreadPool;
//! use std::sync::mpsc::channel;
//!
//! let n_workers = 4;
//!
//! const TARGET_SQUARE: u64 = 1 << 50;
//!
//! let mut pool = ThreadPool::new(n_workers);
//!
//! // channel to send back results
//! let (tx, rx) = channel();
//!
//! for start in 0..0xffff_ffff_ffff {
//!     if let Ok(result) = rx.try_recv() {
//!         println!("Result found: {0:x}*{0:x} = {1:x}", result, TARGET_SQUARE);
//!         break;
//!     }
//!     let start = start << 16;
//!     let end = start + 0xffff;
//!     let range = start..end;
//!     let tx = tx.clone();
//!     let job = move || {
//!         for i in range {
//!             if i*i == TARGET_SQUARE {
//!                 tx.send(i).unwrap();
//!             }
//!         }
//!     };
//!
//!     pool.execute(job);
//! }
//!
//! ```
//!

#![feature(fnbox)]
#![feature(box_syntax)]
#![deny(bare_trait_objects)]
#![deny(unused_must_use)]
#![deny(unused_extern_crates)]
#![deny(missing_docs)]

use std::thread;
use std::sync::mpsc;
use std::boxed::FnBox;


type Job = Box<dyn FnBox() + Send + 'static>;

struct Scheduler {
    tx: mpsc::Sender<Option<Job>>
}


impl Scheduler {
    pub fn submit<F>(self, job: F) where F: FnOnce() + Send + 'static {
        self.tx.send(Some(box job)).unwrap();
    }

    fn exit(&mut self) {
        self.tx.send(None).unwrap();
        std::mem::forget(self);
    }
}


/// This is the thread pool.
///
pub struct ThreadPool {
    req_rx: mpsc::Receiver<Scheduler>,
    threads: Vec<thread::JoinHandle<()>>,
}

impl ThreadPool {
    fn worker(id: usize, req_tx: mpsc::Sender<Scheduler>) {
        loop {
            // send request
                let (job_tx, job_rx) = mpsc::channel();
                let sch = Scheduler {
                    tx: job_tx,
                };

                req_tx.send(sch).unwrap();

            if let Some(f) = job_rx.recv().unwrap() {
                f();
            } else {
                println!("Worker {} exiting.", id);
                break;
            }
        }
    }

    /// Create a new ThreadPool.
    ///
    /// # Example
    /// ```
    /// use sync_threadpool::ThreadPool;
    ///
    /// let pool = ThreadPool::new(4);
    /// ```
    pub fn new(n: usize) -> Self {
        let (tx, rx) = mpsc::channel();

        let mut threads = Vec::with_capacity(n);
        for id in 0..n {
            let tx = tx.clone();
            let t = thread::spawn(move || Self::worker(id, tx));
            threads.push(t);
        }

        Self {
            req_rx: rx,
            threads: threads
        }
    }

    fn get_scheduler(&mut self) -> Scheduler {
        self.req_rx.recv().unwrap()
    }

    /// Submit a job to the threadpool. This method blocks until a worker becomes idle.
    pub fn execute<F>(&mut self,  job: F) where F: FnOnce() + Send + 'static {
        self.get_scheduler().submit(job)
    }
}


impl Drop for ThreadPool {
    fn drop(&mut self) {
        let num_threads = self.threads.len();
        for mut sch in self.req_rx.iter().take(num_threads) {
            sch.exit();
        }

        let threads = std::mem::replace(&mut self.threads, Vec::new());
        for t in threads {
            t.join().unwrap();
        }
    }
}

