use crate::arch::Detour;
use crate::error::Result;

/// A raw detour.
#[derive(Debug)]
pub struct RawDetour(Detour);

// TODO: stop all threads in target during patch?
impl RawDetour {
    /// Constructs a new inline detour patcher.
    ///
    /// The hook is disabled by default. Even when this function is succesful,
    /// there is no guaranteee that the detour function will actually get called
    /// when the target function gets called. An invocation of the target
    /// function might for example get inlined in which case it is impossible to
    /// hook at runtime.
    pub unsafe fn new(target: *const (), detour: *const ()) -> Result<Self> {
        Detour::new(target, detour).map(RawDetour)
    }

    /// Enables the detour.
    pub unsafe fn enable(&self) -> Result<()> {
        self.0.enable()
    }

    /// Disables the detour.
    pub unsafe fn disable(&self) -> Result<()> {
        self.0.disable()
    }

    /// Returns whether the detour is enabled or not.
    pub fn is_enabled(&self) -> bool {
        self.0.is_enabled()
    }

    /// Returns a reference to the generated trampoline.
    pub fn trampoline(&self) -> &() {
        self.0.trampoline()
    }
}
