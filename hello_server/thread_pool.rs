//! Thread pool that joins all thread when dropped.

// NOTE: Crossbeam channels are MPMC, which means that you don't need to wrap the receiver in
// Arc<Mutex<..>>. Just clone the receiver and give it to each worker thread.
use crossbeam_channel::{unbounded, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;

struct Job(Box<dyn FnOnce() + Send + 'static>);

#[derive(Debug)]
struct Worker {
    _id: usize,
    thread: Option<thread::JoinHandle<()>>,
}

impl Worker {
    fn new(id: usize, receiver: Receiver<Job>, pool_inner: Arc<ThreadPoolInner>) -> Self {
        let thread = thread::spawn(move || {
            while let Ok(job) = receiver.recv() {
                pool_inner.start_job();
                (job.0)();
                pool_inner.finish_job();
            }
        });

        Worker {
            _id: id,
            thread: Some(thread),
        }
    }
}

impl Drop for Worker {
    /// When dropped, the thread's `JoinHandle` must be `join`ed.  If the worker panics, then this
    /// function should panic too.
    ///
    /// NOTE: The thread is detached if not `join`ed explicitly.
    fn drop(&mut self) {
        if let Some(handle) = self.thread.take() {
            handle.join().unwrap();
        }
    }
}

/// Internal data structure for tracking the current job status. This is shared by worker closures
/// via `Arc` so that the workers can report to the pool that it started/finished a job.
#[derive(Debug, Default)]
struct ThreadPoolInner {
    job_count: Mutex<usize>,
    empty_condvar: Condvar,
}

impl ThreadPoolInner {
    fn new() -> Self {
        Self {
            job_count: Mutex::new(0),
            empty_condvar: Condvar::new(),
        }
    }

    /// Increment the job count.
    fn start_job(&self) {
        let mut count = self.job_count.lock().unwrap();
        *count += 1;
    }

    /// Decrement the job count.
    fn finish_job(&self) {
        let mut count = self.job_count.lock().unwrap();
        *count -= 1;
        self.empty_condvar.notify_all();
    }

    /// Wait until the job count becomes 0.
    ///
    /// NOTE: We can optimize this function by adding another field to `ThreadPoolInner`, but let's
    /// not care about that in this homework.
    fn wait_empty(&self) {
        let mut count = self.job_count.lock().unwrap();
        while *count > 0 {
            println!("{}", count);
            count = self.empty_condvar.wait(count).unwrap();
        }
    }
}

/// Thread pool.
#[derive(Debug)]
pub struct ThreadPool {
    _workers: Vec<Worker>,
    job_sender: Option<Sender<Job>>,
    pool_inner: Arc<ThreadPoolInner>,
}

impl ThreadPool {
    /// Create a new ThreadPool with `size` threads.
    ///
    /// # Panics
    ///
    /// Panics if `size` is 0.
    pub fn new(size: usize) -> Self {
        assert!(size > 0);

        let (sender, receiver) = unbounded();
        let mut workers = Vec::with_capacity(size);
        let pool_inner = Arc::new(ThreadPoolInner::new());
        for id in 0..size {
            let worker = Worker::new(id, receiver.clone(), Arc::clone(&pool_inner));
            workers.push(worker);
        }

        Self {
            _workers: workers,
            job_sender: Some(sender),
            pool_inner,
        }
    }

    /// Execute a new job in the thread pool.
    pub fn execute<F>(&self, f: F)
    where
        F: FnOnce() + Send + 'static,
    {
        if let Some(ref sender) = self.job_sender {
            sender.send(Job(Box::new(f))).unwrap();
        }
    }

    /// Block the current thread until all jobs in the pool have been executed.
    ///
    /// NOTE: This method has nothing to do with `JoinHandle::join`.
    pub fn join(&self) {
        self.pool_inner.wait_empty()
    }
}

impl Drop for ThreadPool {
    /// When dropped, all worker threads' `JoinHandle` must be `join`ed. If the thread panicked,
    /// then this function should panic too.
    fn drop(&mut self) {
        self.job_sender = None;

        for worker in &mut self._workers {
            if let Some(thread) = worker.thread.take() {
                thread.join().unwrap();
            }
        }
    }
}
