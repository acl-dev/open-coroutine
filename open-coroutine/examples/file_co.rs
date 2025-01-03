use open_coroutine::{task, JoinHandle};
use std::fs::File;
use std::io::{Error, IoSlice, IoSliceMut, Read, Result, Seek, SeekFrom, Write};
use std::time::Duration;

#[open_coroutine::main(event_loop_size = 1, max_size = 1)]
pub fn main() -> Result<()> {
    let join_handle: JoinHandle<Result<()>> = task!(
        |_| {
            const HELLO: &str = "Hello World!";

            // Write
            let mut tmpfile: File = tempfile::tempfile()?;
            assert_eq!(HELLO.len(), tmpfile.write(HELLO.as_ref())?);
            // Seek to start
            tmpfile.seek(SeekFrom::Start(0))?;
            // Read
            let mut buf = String::new();
            assert_eq!(HELLO.len(), tmpfile.read_to_string(&mut buf)?);
            assert_eq!(HELLO, buf);

            // Seek to start
            tmpfile.seek(SeekFrom::Start(0))?;

            // Write multiple
            let ioslices = [IoSlice::new(HELLO.as_ref()), IoSlice::new(HELLO.as_ref())];
            assert_eq!(HELLO.len() * 2, tmpfile.write_vectored(&ioslices)?);
            // Seek to start
            tmpfile.seek(SeekFrom::Start(0))?;
            // Read multiple
            let mut buf1 = [0; HELLO.len()];
            let mut buf2 = [0; HELLO.len()];
            let mut ioslicemuts = [IoSliceMut::new(&mut buf1), IoSliceMut::new(&mut buf2)];
            assert_eq!(HELLO.len() * 2, tmpfile.read_vectored(&mut ioslicemuts)?);
            assert_eq!(HELLO, unsafe { std::str::from_utf8_unchecked(&mut buf1) });
            assert_eq!(HELLO, unsafe { std::str::from_utf8_unchecked(&mut buf2) });

            Ok(())
        },
        ()
    );
    if let Some(r) = join_handle.timeout_join(Duration::from_secs(30))? {
        return r;
    }
    Err(Error::new(
        std::io::ErrorKind::Other,
        "Failed to join the task",
    ))
}
