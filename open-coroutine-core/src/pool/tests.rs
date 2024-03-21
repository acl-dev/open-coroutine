use super::*;
use crate::coroutine::suspender::SimpleDelaySuspender;

#[test]
fn test_simple() {
    let task_name = "test_simple";
    let pool = CoroutinePoolImpl::default();
    pool.set_max_size(1);
    assert!(!pool.has_task());
    _ = pool.submit(
        Some(String::from("test_panic")),
        |_, _| panic!("test panic, just ignore it"),
        None,
    );
    assert!(pool.has_task());
    let name = pool.submit(
        Some(String::from(task_name)),
        |_, _| {
            println!("2");
            Some(2)
        },
        None,
    );
    assert_eq!(task_name, name.get_name().unwrap());
    _ = pool.try_schedule_task();
    assert_eq!(
        Some((
            String::from("test_panic"),
            Err("test panic, just ignore it")
        )),
        pool.try_get_task_result("test_panic")
    );
    assert_eq!(
        Some((String::from(task_name), Ok(Some(2)))),
        pool.try_get_task_result(task_name)
    );
}

#[test]
fn test_suspend() -> std::io::Result<()> {
    let pool = CoroutinePoolImpl::default();
    pool.set_max_size(2);
    _ = pool.submit(
        None,
        |_, param| {
            println!("[coroutine] delay");
            if let Some(suspender) = SchedulableSuspender::current() {
                suspender.delay(Duration::from_millis(100));
            }
            println!("[coroutine] back");
            param
        },
        None,
    );
    _ = pool.submit(
        None,
        |_, _| {
            println!("middle");
            Some(1)
        },
        None,
    );
    pool.try_schedule_task()?;
    std::thread::sleep(Duration::from_millis(200));
    pool.try_schedule_task()
}

#[test]
fn test_wait() {
    let task_name = "test_wait";
    let pool = CoroutinePoolImpl::default();
    pool.set_max_size(1);
    assert!(!pool.has_task());
    let name = pool.submit(
        Some(String::from(task_name)),
        |_, _| {
            println!("2");
            Some(2)
        },
        None,
    );
    assert_eq!(task_name, name.get_name().unwrap());
    assert_eq!(None, pool.try_get_task_result(task_name));
    match pool.wait_result(task_name, Duration::from_millis(100)) {
        Ok(_) => panic!(),
        Err(_) => {}
    }
    assert_eq!(None, pool.try_get_task_result(task_name));
    _ = pool.try_schedule_task();
    match pool.wait_result(task_name, Duration::from_secs(100)) {
        Ok(v) => assert_eq!(Some((String::from(task_name), Ok(Some(2)))), v),
        Err(e) => panic!("{e}"),
    }
}

#[test]
fn test_co_simple() -> std::io::Result<()> {
    let scheduler = SchedulerImpl::default();
    _ = scheduler.submit_co(
        |_, _| {
            let task_name = "test_co_simple";
            let pool = CoroutinePoolImpl::default();
            pool.set_max_size(1);
            let result = pool.submit_and_wait(
                Some(String::from(task_name)),
                |_, _| Some(1),
                None,
                Duration::from_secs(1),
            );
            assert_eq!(
                Some((String::from(task_name), Ok(Some(1)))),
                result.unwrap()
            );
            None
        },
        None,
    )?;
    scheduler.try_schedule()
}

#[test]
fn test_nest() {
    let pool = Arc::new(CoroutinePoolImpl::default());
    pool.set_max_size(1);
    let arc = pool.clone();
    _ = pool.submit_and_wait(
        None,
        move |_, _| {
            println!("start");
            _ = arc.submit_and_wait(
                None,
                |_, _| {
                    println!("middle");
                    None
                },
                None,
                Duration::from_secs(1),
            );
            println!("end");
            None
        },
        None,
        Duration::from_secs(1),
    );
}
