include!("../examples/preemptive.rs");

#[cfg(not(windows))]
#[test]
fn socket_co() -> std::io::Result<()> {
    main()
}
