pub mod set_jmp;

pub mod context;

#[repr(C)]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum State {
    ///协程被创建
    Created,
    ///等待运行
    Ready,
    ///运行中
    Running,
    ///被挂起，参数为延迟的时间，单位ns
    Suspend(u64),
    ///执行系统调用
    SystemCall,
    ///栈扩/缩容时
    CopyStack,
    ///执行用户函数完成
    Finished,
}
