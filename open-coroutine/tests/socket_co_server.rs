include!("../examples/socket_co_server.rs");

#[test]
fn socket_co_server() -> std::io::Result<()> {
    main()
}
