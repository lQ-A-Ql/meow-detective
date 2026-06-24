//! Platform-specific security helpers.
//!
//! Currently implements Windows DACL restriction so that a file is only
//! accessible by the user running the current process.

use std::path::Path;

#[cfg(windows)]
pub fn restrict_file_to_current_user(path: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{LocalFree, HANDLE, HLOCAL};
    use windows::Win32::Security::Authorization::{SetNamedSecurityInfoW, SE_FILE_OBJECT};
    use windows::Win32::Security::{
        AddAccessAllowedAce, GetLengthSid, GetTokenInformation, InitializeAcl, IsValidSid,
        TokenUser, ACCESS_ALLOWED_ACE, ACL, ACL_REVISION, DACL_SECURITY_INFORMATION,
        PROTECTED_DACL_SECURITY_INFORMATION, PSID, TOKEN_QUERY, TOKEN_USER,
    };
    use windows::Win32::Storage::FileSystem::FILE_ALL_ACCESS;
    use windows::Win32::System::Memory::{LocalAlloc, LPTR};
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    fn win_err_to_io(e: windows::core::Error) -> std::io::Error {
        std::io::Error::from_raw_os_error(e.code().0)
    }

    // SAFETY: All Win32 API calls use stack-allocated or LocalAlloc'd buffers
    // whose sizes are queried from the Windows API itself (GetTokenInformation
    // reports the required buffer size). The HANDLE obtained from
    // OpenProcessToken is a pseudo-handle from GetCurrentProcess, so it does
    // not require explicit close. All error paths invoke the `cleanup` closure
    // to free LocalAlloc'd memory before returning.
    unsafe {
        // Open the current process token with query access.
        let mut token = HANDLE::default();
        OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).map_err(win_err_to_io)?;

        // Determine the size needed for TOKEN_USER.
        let mut info_len: u32 = 0;
        let _ = GetTokenInformation(token, TokenUser, None, 0, &mut info_len);
        if info_len == 0 {
            return Err(std::io::Error::last_os_error());
        }

        // Allocate and retrieve TOKEN_USER.
        let user_mem: HLOCAL = LocalAlloc(LPTR, info_len as usize).map_err(win_err_to_io)?;
        let user_ptr = user_mem.0;
        GetTokenInformation(token, TokenUser, Some(user_ptr), info_len, &mut info_len).map_err(
            |e| {
                let _ = LocalFree(user_mem);
                win_err_to_io(e)
            },
        )?;

        let token_user = user_ptr as *const TOKEN_USER;
        let user_sid = PSID((*token_user).User.Sid.0);

        if !IsValidSid(user_sid).as_bool() {
            let _ = LocalFree(user_mem);
            return Err(std::io::Error::other("current user SID is invalid"));
        }

        // Build a minimal ACL allowing only the current user.
        let sid_len = GetLengthSid(user_sid) as usize;
        let acl_len = std::mem::size_of::<ACL>() + std::mem::size_of::<ACCESS_ALLOWED_ACE>()
            - std::mem::size_of::<u32>()
            + sid_len;
        let acl_mem: HLOCAL = LocalAlloc(LPTR, acl_len).map_err(|e| {
            let _ = LocalFree(user_mem);
            win_err_to_io(e)
        })?;
        let acl = acl_mem.0 as *mut ACL;

        let cleanup = || {
            let _ = LocalFree(acl_mem);
            let _ = LocalFree(user_mem);
        };

        InitializeAcl(acl, acl_len as u32, ACL_REVISION).map_err(|e| {
            cleanup();
            win_err_to_io(e)
        })?;

        AddAccessAllowedAce(acl, ACL_REVISION, FILE_ALL_ACCESS.0, user_sid).map_err(|e| {
            cleanup();
            win_err_to_io(e)
        })?;

        // Apply the new protected DACL to the file.
        let wide: Vec<u16> = path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let info = DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION;
        let ret = SetNamedSecurityInfoW(
            PCWSTR::from_raw(wide.as_ptr()),
            SE_FILE_OBJECT,
            info,
            None,
            None,
            Some(acl),
            None,
        );

        cleanup();

        if ret.0 != 0 {
            return Err(std::io::Error::from_raw_os_error(ret.0 as i32));
        }

        Ok(())
    }
}

#[cfg(not(windows))]
pub fn restrict_file_to_current_user(_path: &Path) -> std::io::Result<()> {
    // ACLs are platform-specific; on non-Windows the caller should rely on
    // filesystem permissions or sandboxing.
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn restrict_succeeds_for_regular_file() {
        let dir = std::env::temp_dir().join(format!(
            "forensics-platform-security-test-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("restricted.txt");
        {
            let mut f = std::fs::File::create(&file).unwrap();
            f.write_all(b"secret").unwrap();
        }

        let result = restrict_file_to_current_user(&file);
        std::fs::remove_dir_all(&dir).ok();

        // On Windows the ACL operation must succeed; on other platforms it is a no-op.
        assert!(
            result.is_ok(),
            "restrict_file_to_current_user failed: {result:?}"
        );
    }
}
