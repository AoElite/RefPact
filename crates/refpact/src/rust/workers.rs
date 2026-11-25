//! Threads that do jobs for the thread that started them.
//!
//! A syntax tree cannot leave the thread that parsed it: the positions of its tokens are
//! known only to that thread. So a thread that parses a file must also do all later work on
//! that file. Each thread here has its own state, made when the thread starts, and takes
//! jobs from one queue that all threads share. This is the only module that starts threads.

use std::any::Any;
use std::panic::{self, AssertUnwindSafe};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;

/// The most threads of one pool. More do not help: one thread orders all the work.
const MAX_THREADS: usize = 8;
/// The stack of a thread. A parser recurses once for each level of nesting in the source.
const STACK_SIZE: usize = 8 * 1024 * 1024;

type Answer<Reply> = Result<Reply, Box<dyn Any + Send>>;

enum Message<Job> {
    Job(Job),
    /// The thread ends after this job.
    Last(Job),
}

/// Starts a thread for one job that the caller does not wait for at once: to read what a
/// program writes while the caller waits for the program, or to wait for a limit.
pub(crate) fn in_background<T: Send + 'static>(
    job: impl FnOnce() -> T + Send + 'static,
) -> thread::JoinHandle<T> {
    thread::spawn(job)
}

/// The threads of one call of [`with_pool`].
pub(crate) struct Pool<'scope, 'env, Job, Reply, Start> {
    scope: &'scope thread::Scope<'scope, 'env>,
    start: &'env Start,
    limit: usize,
    threads: usize,
    jobs: mpsc::Sender<Message<Job>>,
    queue: Arc<Mutex<mpsc::Receiver<Message<Job>>>>,
    answers: mpsc::Sender<Answer<Reply>>,
    inbox: mpsc::Receiver<Answer<Reply>>,
}

/// Runs `body` with a pool. Each thread of the pool calls `start` one time and then gives
/// each of its jobs to the function that `start` returned. All threads end before this
/// function returns.
pub(crate) fn with_pool<Job, Reply, Start, Work, T>(
    start: Start,
    body: impl for<'scope, 'env> FnOnce(&mut Pool<'scope, 'env, Job, Reply, Start>) -> T,
) -> T
where
    Job: Send,
    Reply: Send,
    Start: Fn() -> Work + Sync,
    Work: FnMut(Job) -> Reply,
{
    let limit = thread::available_parallelism()
        .map_or(1, usize::from)
        .min(MAX_THREADS);
    thread::scope(|scope| {
        let (answers, inbox) = mpsc::channel();
        let (jobs, queue) = mpsc::channel();
        let queue = Arc::new(Mutex::new(queue));
        let mut pool = Pool {
            scope,
            start: &start,
            limit,
            threads: 0,
            jobs,
            queue,
            answers,
            inbox,
        };
        body(&mut pool)
    })
}

impl<'scope, 'env, Job, Reply, Start, Work> Pool<'scope, 'env, Job, Reply, Start>
where
    Job: Send + 'env,
    Reply: Send + 'env,
    Start: Fn() -> Work + Sync,
    Work: FnMut(Job) -> Reply,
{
    /// Gives a job to the threads; one of them does it. A new thread starts while the pool
    /// has fewer than its limit. Each job that is given has one reply.
    ///
    /// Returns the job when the system gives no thread at all. The caller then does the job.
    pub fn give(&mut self, job: Job) -> Result<(), Job> {
        if self.threads < self.limit {
            let queue = Arc::clone(&self.queue);
            let answers = self.answers.clone();
            let start = self.start;
            let spawned =
                thread::Builder::new()
                    .stack_size(STACK_SIZE)
                    .spawn_scoped(self.scope, move || {
                        let mut work = start();
                        loop {
                            // The lock is held only while this thread waits for a job.
                            let message = match queue.lock() {
                                Ok(queue) => queue.recv(),
                                Err(_) => return,
                            };
                            let (job, last) = match message {
                                Ok(Message::Job(job)) => (job, false),
                                Ok(Message::Last(job)) => (job, true),
                                Err(_) => return,
                            };
                            // A panic goes to the thread that waits for the reply. Without this,
                            // that thread waits forever.
                            let answer = panic::catch_unwind(AssertUnwindSafe(|| work(job)));
                            let failed = answer.is_err();
                            if answers.send(answer).is_err() || failed || last {
                                return;
                            }
                        }
                    });
            if spawned.is_ok() {
                self.threads += 1;
            }
        }
        if self.threads == 0 {
            return Err(job);
        }
        self.jobs
            .send(Message::Job(job))
            .map_err(|returned| match returned.0 {
                Message::Job(job) | Message::Last(job) => job,
            })
    }

    /// Gives one job to each thread, as the last job of the thread. Returns the number of
    /// replies that will come.
    pub fn give_last(&mut self, job: impl Fn() -> Job) -> usize {
        let threads = std::mem::take(&mut self.threads);
        (0..threads)
            .filter(|_| self.jobs.send(Message::Last(job())).is_ok())
            .count()
    }

    /// Waits for one reply. A panic of a thread continues in the thread that calls this.
    pub fn reply(&self) -> Reply {
        match self.inbox.recv() {
            Ok(Ok(reply)) => reply,
            Ok(Err(payload)) => panic::resume_unwind(payload),
            // The pool holds a sender, so the channel does not close.
            Err(_) => unreachable!("the pool holds a sender of the replies"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_job_has_one_reply_and_each_thread_keeps_its_own_state() {
        // The state of a thread is the number of jobs that it did.
        let start = || {
            let mut done = 0;
            move |job: u32| {
                done += 1;
                (job * 2, done)
            }
        };
        let (mut doubled, last) = with_pool(start, |pool| {
            let given: Vec<u32> = (0..100).filter(|job| pool.give(*job).is_ok()).collect();
            let doubled: Vec<u32> = given.iter().map(|_| pool.reply().0).collect();
            let threads = pool.give_last(|| 1000);
            let last: Vec<(u32, u32)> = (0..threads).map(|_| pool.reply()).collect();
            (doubled, last)
        });
        doubled.sort_unstable();
        assert_eq!(doubled, (0..100).map(|job| job * 2).collect::<Vec<u32>>());
        // Each thread got the last job one time, and the threads did all jobs between them.
        assert!(!last.is_empty() && last.len() <= MAX_THREADS);
        assert!(last.iter().all(|(reply, _)| *reply == 2000));
        assert_eq!(
            last.iter().map(|(_, done)| done).sum::<u32>(),
            100 + last.len() as u32
        );
    }

    #[test]
    fn a_panic_of_a_job_continues_in_the_thread_that_waits() {
        let start = || |job: u32| if job == 3 { panic!("job three") } else { job };
        let result = panic::catch_unwind(|| {
            with_pool(start, |pool| {
                let given = (0..8).filter(|job| pool.give(*job).is_ok()).count();
                (0..given).map(|_| pool.reply()).sum::<u32>()
            })
        });
        let payload = result.expect_err("the panic of the job reaches the caller");
        assert_eq!(payload.downcast_ref::<&str>(), Some(&"job three"));
    }
}
