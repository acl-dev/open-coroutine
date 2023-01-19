use once_cell::sync::Lazy;

static INSTANCE: Lazy<ThreadPool> = Lazy::new(ThreadPool::new);

pub fn get_instance() -> &'static ThreadPool {
    &INSTANCE
}

pub struct ThreadPool(threadpool::ThreadPool);

impl ThreadPool {
    fn new() -> Self {
        ThreadPool(threadpool::ThreadPool::with_name(
            "open-coroutine".into(),
            num_cpus::get(),
        ))
    }

    pub fn execute<F>(&self, job: F)
    where
        F: FnOnce() + Send + 'static,
    {
        self.0.execute(job)
    }

    pub fn join(&self) {
        self.0.join()
    }
}

unsafe impl Sync for ThreadPool {}
