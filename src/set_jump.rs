#[cfg(target_arch = "x86_64")]
const _JBLEN: usize = (9 * 2) + 3 + 16;

#[cfg(target_arch = "x86")]
const _JBLEN: usize = 18;

#[cfg(target_arch = "aarch64")]
const _JBLEN: usize = (14 + 8 + 2) * 2;

#[cfg(target_arch = "arm")]
const _JBLEN: usize = 10 + 16 + 2;

pub type JmpBuf = [libc::c_int; _JBLEN];

extern "C" {

    pub fn setjmp(env: *mut JmpBuf) -> libc::c_int;

    pub fn longjmp(env: *mut JmpBuf, arg: libc::c_int);
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe fn func(mut buf: JmpBuf) {
        println!("func");
        longjmp(&mut buf, 1);
        println!("you will never see this because of longjmp");
    }

    #[test]
    fn test() {
        unsafe {
            let mut buf: JmpBuf = std::mem::zeroed();
            if setjmp(&mut buf) != 0 {
                println!("back to main");
            } else {
                println!("first time through");
                func(buf);
            }
        }
    }
}
