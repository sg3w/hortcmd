//! Restrict the staging directory before any archive data is created. The
//! protected DACL grants full access only to the owner and LocalSystem and is
//! inherited by new files/subdirectories. All opens remain handle-relative.
use cap_std::fs::{Dir, OpenOptions, OpenOptionsExt};
use std::io;
use std::os::windows::io::AsRawHandle;
use windows::core::w;
use windows::Win32::Foundation::{LocalFree, BOOL, HANDLE, HLOCAL};
use windows::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, SetSecurityInfo, SDDL_REVISION_1,
    SE_FILE_OBJECT,
};
use windows::Win32::Security::{
    GetSecurityDescriptorDacl, DACL_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION,
    PSECURITY_DESCRIPTOR, PSID,
};
use windows::Win32::Storage::FileSystem::{FILE_FLAG_BACKUP_SEMANTICS, WRITE_DAC};

pub(super) fn protect(root: &Dir, name: &str) -> io::Result<()> {
    let file = root.open_with(
        name,
        OpenOptions::new()
            .access_mode(WRITE_DAC.0)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS.0),
    )?;
    let mut descriptor = PSECURITY_DESCRIPTOR::default();
    // SAFETY: the descriptor is allocated by Windows, kept alive until after
    // SetSecurityInfo copies its DACL, and then freed exactly once with LocalFree.
    unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            w!("D:P(A;OICI;FA;;;OW)(A;OICI;FA;;;SY)"),
            SDDL_REVISION_1,
            &mut descriptor,
            None,
        )
        .map_err(io::Error::other)?;
        let result = (|| -> windows::core::Result<()> {
            let mut present = BOOL::default();
            let mut defaulted = BOOL::default();
            let mut acl = std::ptr::null_mut();
            GetSecurityDescriptorDacl(descriptor, &mut present, &mut acl, &mut defaulted)?;
            SetSecurityInfo(
                HANDLE(file.as_raw_handle()),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                PSID::default(),
                PSID::default(),
                Some(acl),
                None,
            )
            .ok()
        })();
        let _ = LocalFree(HLOCAL(descriptor.0));
        result.map_err(io::Error::other)
    }
}
