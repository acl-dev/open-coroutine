include!("../examples/socket_not_co.rs");

#[test]
fn socket_not_co() -> std::io::Result<()> {
    main()
}
