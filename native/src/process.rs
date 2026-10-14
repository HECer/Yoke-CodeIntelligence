use anyhow::Result;
use process_wrap::std::{StdChildWrapper, StdCommandWrap};
use std::{
    ops::{Deref, DerefMut},
    process::Command,
};

pub struct ManagedChild {
    inner: Box<dyn StdChildWrapper>,
}
impl ManagedChild {
    pub fn spawn(command: Command) -> Result<Self> {
        let mut wrapper = StdCommandWrap::from(command);
        #[cfg(windows)]
        wrapper.wrap(process_wrap::std::JobObject);
        #[cfg(unix)]
        wrapper.wrap(process_wrap::std::ProcessGroup::leader());
        Ok(Self {
            inner: wrapper.spawn()?,
        })
    }
}
impl Deref for ManagedChild {
    type Target = dyn StdChildWrapper;
    fn deref(&self) -> &Self::Target {
        &*self.inner
    }
}
impl DerefMut for ManagedChild {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut *self.inner
    }
}
impl Drop for ManagedChild {
    fn drop(&mut self) {
        if self.inner.start_kill().is_ok() {
            let _ = self.inner.inner_mut().wait();
        }
    }
}
