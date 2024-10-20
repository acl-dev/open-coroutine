include!("../examples/file_co.rs");

// The implementation of rust std is inconsistent between unix and windows.
#[cfg(not(windows))]
#[test]
fn file_co() -> Result<()> {
    main()
}
