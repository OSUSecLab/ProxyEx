use std::thread;

use crossbeam::channel::{self, Receiver, Sender};
use rayon::{ThreadPool, ThreadPoolBuilder};

pub struct FIFOTaskPool<R> {
    pool: ThreadPool,
    result_queue_tx: Sender<Receiver<R>>,
    result_thread: std::thread::JoinHandle<()>,
}

impl<R: Send + 'static> FIFOTaskPool<R> {
    pub fn new(result_tx: Sender<R>, n_threads: usize) -> Self {
        let pool = ThreadPoolBuilder::new()
            .num_threads(n_threads)
            .build()
            .unwrap();

        let (result_queue_tx, result_queue_rv) = channel::bounded(n_threads * 2);

        let result_thread = thread::spawn(move || loop {
            let r_rv: Receiver<R> = match result_queue_rv.recv() {
                Ok(r) => r,
                Err(_) => break,
            };
            let r = match r_rv.recv() {
                Ok(r) => r,
                Err(_) => break,
            };
            match result_tx.send(r) {
                Ok(_) => continue,
                Err(_) => break,
            }
        });
        Self {
            pool,
            result_queue_tx,
            result_thread,
        }
    }

    pub fn add_task<F: FnOnce() -> R>(&self, op: F)
    where
        F: Send + 'static,
        R: 'static,
    {
        let (result_tx, result_rv) = channel::bounded(1);
        self.result_queue_tx
            .send(result_rv)
            .expect("impossible: result queue is disconnected");
        self.pool.spawn(move || {
            let result = op();
            let _ = result_tx.send(result); // no one will disconnect result_tx, so no error is possible.
        });
    }

    pub fn close(self) {
        drop(self.result_queue_tx);
        self.result_thread
            .join()
            .expect("bug: result thread is dead");
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_ordered_result() {
        let (result_tx, result_rv) = crossbeam::channel::bounded(3);
        let pool = super::FIFOTaskPool::new(result_tx, 3);
        pool.add_task(|| {
            std::thread::sleep(std::time::Duration::from_millis(200));
            1
        });
        pool.add_task(|| {
            std::thread::sleep(std::time::Duration::from_millis(100));
            2
        });
        pool.add_task(|| {
            std::thread::sleep(std::time::Duration::from_millis(50));
            3
        });
        let r = result_rv.recv().unwrap();
        assert_eq!(r, 1);
        let r = result_rv.recv().unwrap();
        assert_eq!(r, 2);
        let r = result_rv.recv().unwrap();
        assert_eq!(r, 3);

        pool.close();
    }

    #[test]
    fn test_task_parallel() {
        let (result_tx, result_rv) = crossbeam::channel::bounded(3);
        let pool = super::FIFOTaskPool::new(result_tx, 3);
        let start_at = std::time::Instant::now();
        pool.add_task(|| {
            std::thread::sleep(std::time::Duration::from_millis(100));
            1
        });
        pool.add_task(|| {
            std::thread::sleep(std::time::Duration::from_millis(100));
            2
        });
        pool.add_task(|| {
            std::thread::sleep(std::time::Duration::from_millis(100));
            3
        });
        result_rv.recv().unwrap();
        result_rv.recv().unwrap();
        result_rv.recv().unwrap();

        let time_elapsed = std::time::Instant::now() - start_at;
        assert!(time_elapsed < std::time::Duration::from_millis(200));
    }

    #[test]
    fn test_close_before_task_finish() {
        let (result_tx, _) = crossbeam::channel::bounded(3);
        let pool = super::FIFOTaskPool::new(result_tx, 3);
        pool.add_task(|| {
            std::thread::sleep(std::time::Duration::from_millis(200));
            1
        });
        let start_at = std::time::Instant::now();
        pool.close();
        let time_elapsed = std::time::Instant::now() - start_at;
        assert!(time_elapsed > std::time::Duration::from_millis(200))
    }
}
