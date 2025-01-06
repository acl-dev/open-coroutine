include!("../examples/preemptive.rs");

#[cfg(not(windows))]
#[test]
fn preemptive() -> std::io::Result<()> {
    main()
}
