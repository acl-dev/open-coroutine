#[cfg(test)]
mod tests {
    use corosensei::{Coroutine, CoroutineResult, Yielder};

    #[test]
    fn test() {
        println!("[main] creating coroutine");

        let mut main_coroutine = Coroutine::new(|main_yielder, input| {
            println!("[main coroutine] launched");
            let main_yielder =
                unsafe { std::ptr::read_unaligned(main_yielder as *const Yielder<(), i32>) };

            let mut coroutine2 = Coroutine::new(move |_: &Yielder<(), ()>, input| {
                println!("[coroutine2] launched");
                main_yielder.suspend(1);
                2
            });

            let mut coroutine1 = Coroutine::new(move |_: &Yielder<(), ()>, input| {
                println!("[coroutine1] launched");
                coroutine2.resume(());
            });
            coroutine1.resume(());
            3
        });

        println!("[main] resuming coroutine");
        match main_coroutine.resume(()) {
            CoroutineResult::Yield(i) => println!("[main] got {:?} from coroutine", i),
            CoroutineResult::Return(r) => {
                println!("[main] got result {:?} from coroutine", r);
            }
        }

        println!("[main] exiting");
    }
}
