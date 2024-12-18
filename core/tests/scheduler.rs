use open_coroutine_core::scheduler::Scheduler;
use std::time::Duration;

#[test]
fn scheduler_basic() -> std::io::Result<()> {
    let mut scheduler = Scheduler::default();
    _ = scheduler.submit_co(
        |_, _| {
            println!("1");
            None
        },
        None,
        None,
    )?;
    _ = scheduler.submit_co(
        |_, _| {
            println!("2");
            None
        },
        None,
        None,
    )?;
    scheduler.try_schedule()
}

#[test]
fn scheduler_backtrace() -> std::io::Result<()> {
    let mut scheduler = Scheduler::default();
    _ = scheduler.submit_co(|_, _| None, None, None)?;
    _ = scheduler.submit_co(
        |_, _| {
            println!("{:?}", backtrace::Backtrace::new());
            None
        },
        None,
        None,
    )?;
    scheduler.try_schedule()
}

#[test]
fn scheduler_suspend() -> std::io::Result<()> {
    let mut scheduler = Scheduler::default();
    _ = scheduler.submit_co(
        |suspender, _| {
            println!("[coroutine1] suspend");
            suspender.suspend();
            println!("[coroutine1] back");
            None
        },
        None,
        None,
    )?;
    _ = scheduler.submit_co(
        |suspender, _| {
            println!("[coroutine2] suspend");
            suspender.suspend();
            println!("[coroutine2] back");
            None
        },
        None,
        None,
    )?;
    scheduler.try_schedule()
}

#[test]
fn scheduler_delay() -> std::io::Result<()> {
    let mut scheduler = Scheduler::default();
    _ = scheduler.submit_co(
        |suspender, _| {
            println!("[coroutine] delay");
            suspender.delay(Duration::from_millis(100));
            println!("[coroutine] back");
            None
        },
        None,
        None,
    )?;
    scheduler.try_schedule()?;
    std::thread::sleep(Duration::from_millis(100));
    scheduler.try_schedule()
}

#[test]
fn scheduler_listener() -> std::io::Result<()> {
    use open_coroutine_core::coroutine::listener::Listener;
    use open_coroutine_core::coroutine::local::CoroutineLocal;
    use open_coroutine_core::scheduler::SchedulableCoroutineState;

    #[derive(Debug, Default)]
    struct TestListener {}
    impl Listener<(), Option<usize>> for TestListener {
        fn on_create(&self, local: &CoroutineLocal, _: usize) {
            println!("{:?}", local);
        }

        fn on_state_changed(
            &self,
            local: &CoroutineLocal,
            old_state: SchedulableCoroutineState,
            new_state: SchedulableCoroutineState,
        ) {
            println!("{} {}->{}", local, old_state, new_state);
        }

        fn on_complete(&self, _: &CoroutineLocal, _: SchedulableCoroutineState, _: Option<usize>) {
            panic!("test on_complete panic, just ignore it");
        }

        fn on_error(&self, _: &CoroutineLocal, _: SchedulableCoroutineState, _: &str) {
            panic!("test on_error panic, just ignore it");
        }
    }

    let mut scheduler = Scheduler::default();
    scheduler.add_listener(TestListener::default());
    scheduler.submit_co(|_, _| panic!("test panic, just ignore it"), None, None)?;
    scheduler.submit_co(
        |_, _| {
            println!("2");
            None
        },
        None,
        None,
    )?;
    scheduler.try_schedule()
}
