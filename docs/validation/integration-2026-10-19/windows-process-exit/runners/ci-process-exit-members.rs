    fn member_handles(&self, end: Instant) -> Result<Vec<OwnedHandle>> {
        const MAX_MEMBERS: usize = 4096;
        #[repr(C)]
        struct ProcessIds {
            assigned: u32,
            listed: u32,
            ids: [usize; MAX_MEMBERS],
        }
        anyhow::ensure!(Instant::now() < end, CleanupUnproven::new("Owned member snapshot exceeded cleanup budget"));
        let mut info = ProcessIds { assigned: 0, listed: 0, ids: [0; MAX_MEMBERS] };
        // The OS list includes nested child jobs. Fixed storage bounds both the
        // protocol and synchronization handles; an incomplete list is not proof.
        if unsafe { QueryInformationJobObject(self.job.as_raw_handle(), 3, std::ptr::from_mut(&mut info).cast(), std::mem::size_of::<ProcessIds>() as u32, std::ptr::null_mut()) } == 0 {
            return Err(CleanupUnproven::new("Owned member list unavailable or over capacity").into());
        }
        anyhow::ensure!(info.assigned == info.listed && info.listed as usize <= MAX_MEMBERS,
            CleanupUnproven::new("Owned member list incomplete or over capacity"));
        let mut handles = Vec::with_capacity(info.listed as usize);
        for id in &info.ids[..info.listed as usize] {
            anyhow::ensure!(Instant::now() < end, CleanupUnproven::new("Owned member snapshot exceeded cleanup budget"));
            let id = u32::try_from(*id).ok().filter(|id| *id != 0)
                .ok_or_else(|| CleanupUnproven::new("Owned member identity unavailable"))?;
            // Synchronization only: never terminate a process by a recycled PID.
            // PID reuse can conservatively extend a wait or refuse proof; it
            // cannot terminate an unrelated process. The job owns termination.
            let handle = unsafe { OpenProcess(0x0010_0000, 0, id) };
            if handle.is_null() {
                // With fixed valid access and a nonzero OS-supplied PID, this
                // error means the process no longer exists. Its object exited.
                if io::Error::last_os_error().raw_os_error() == Some(87) { continue; }
                return Err(CleanupUnproven::new("Owned member synchronization handle unavailable").into());
            }
            // SAFETY: OpenProcess returned a new non-null owned handle.
            handles.push(unsafe { OwnedHandle::from_raw_handle(handle) });
        }
        Ok(handles)
    }
    pub(super) fn terminate_until(&self, end: Instant) -> Result<()> {
        // Retain member identities before asynchronous termination can remove
        // them from job accounting while their process objects remain unsignaled.
        let members = self.member_handles(end);
        // Always request termination, including when member admission refuses.
        let termination_requested = unsafe { TerminateJobObject(self.job.as_raw_handle(), 1) } != 0;
        if !self.attached.load(Ordering::SeqCst)
            && !self.root_exited().map_err(|_| CleanupUnproven::new("Owned root state unavailable"))?
        {
            let root = self.root.get().expect("root_exited checked the retained handle");
            if unsafe { TerminateProcess(root.as_raw_handle(), 1) } == 0
                && !self.root_exited().map_err(|_| CleanupUnproven::new("Owned root state unavailable"))?
            {
                return Err(CleanupUnproven::new("Unassigned owned root termination failed").into());
            }
        }
        let members = members?;
        loop {
            let remaining = end.saturating_duration_since(Instant::now());
            anyhow::ensure!(!remaining.is_zero(), CleanupUnproven::new("Owned group exit exceeded cleanup budget"));
            let root_exited = self.root_exited().map_err(|_| CleanupUnproven::new("Owned root state unavailable"))?;
            let empty = self.empty().map_err(|_| CleanupUnproven::new("Owned job accounting unavailable"))?;
            let mut members_exited = true;
            for member in &members {
                // Live independently owned process handles, zero-time polling.
                match unsafe { WaitForSingleObject(member.as_raw_handle(), 0) } {
                    0 => {},
                    258 => members_exited = false,
                    _ => return Err(CleanupUnproven::new("Owned member exit state unavailable").into()),
                }
            }
            if root_exited && empty && members_exited && Instant::now() < end { return Ok(()); }
            if !termination_requested && !empty { return Err(CleanupUnproven::new("Owned job termination request failed").into()); }
            std::thread::sleep(end.saturating_duration_since(Instant::now()).min(Duration::from_millis(5)));
        }
    }
