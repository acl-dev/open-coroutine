use libc::{sigaction, siginfo_t, ucontext_t, SA_NODEFER, SA_RESTART, SA_SIGINFO, SIGINT};
use std::os::unix::thread::JoinHandleExt;

static mut RUN: bool = true;

unsafe extern "C" fn yields() {
    println!("yielded after signal_handler returned");
    RUN = false;
}

unsafe extern "C" fn signal_handler(signum: i32, _siginfo: &siginfo_t, context: &mut ucontext_t) {
    cfg_if::cfg_if! {
        if #[cfg(all(
            any(target_os = "linux", target_os = "android"),
            target_arch = "x86_64",
        ))] {
            context.uc_mcontext.gregs[libc::REG_RIP as usize] = yields as i64;
        } else if #[cfg(all(
                    any(target_os = "linux", target_os = "android"),
                    target_arch = "x86",
        ))] {
            context.uc_mcontext.gregs[libc::REG_EIP as usize] = yields as i32;
        } else if #[cfg(all(
                    any(target_os = "linux", target_os = "android"),
                    target_arch = "aarch64",
        ))] {
            context.uc_mcontext.pc = yields as libc::c_ulong;
        } else if #[cfg(all(
                    any(target_os = "linux", target_os = "android"),
                    target_arch = "arm",
        ))] {
            context.uc_mcontext.arm_pc = yields as libc::c_ulong;
        } else if #[cfg(all(
                    any(target_os = "linux", target_os = "android"),
                    any(target_arch = "riscv64", target_arch = "riscv32"),
        ))] {
            context.uc_mcontext.__gregs[libc::REG_PC] = yields as libc::c_ulong;
        } else if #[cfg(all(target_vendor = "apple", target_arch = "aarch64"))] {
            (*context.uc_mcontext).__ss.__pc = yields as u64;
        } else if #[cfg(all(target_vendor = "apple", target_arch = "x86_64"))] {
            (*context.uc_mcontext).__ss.__rip = yields as u64;
        } else {
            compile_error!("Unsupported platform");
        }
    }
    println!("Received signal {}", signum);
}

//RUSTFLAGS="--emit asm" cargo build --example change_pc --release 获取优化后的汇编代码
fn main() {
    let handler = std::thread::spawn(|| unsafe {
        let mut sa: sigaction = std::mem::zeroed();
        sa.sa_sigaction = signal_handler as usize;
        libc::sigemptyset(&mut sa.sa_mask);
        sa.sa_flags = SA_SIGINFO | SA_RESTART | SA_NODEFER;
        if sigaction(SIGINT, &sa, std::ptr::null_mut()) == -1 {
            println!("Failed to register handler for SIGINT");
            return;
        }

        while RUN {
            println!("Waiting for signal...");
            libc::sleep(1);
        }
    });
    let pthread = handler.as_pthread_t();
    unsafe {
        libc::sleep(1);
        libc::pthread_kill(pthread, SIGINT);
        libc::sleep(1);
        handler.join().unwrap();
    }
}
