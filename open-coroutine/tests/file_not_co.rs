include!("../examples/file_not_co.rs");

// The implementation of rust std is inconsistent between unix and windows.
#[cfg(not(windows))]
#[test]
fn file_not_co() -> std::io::Result<()> {
    main()
}
