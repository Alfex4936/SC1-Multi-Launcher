use std::ptr;
use windows::core::PWSTR;
use windows::Win32::Foundation::{BOOL, HANDLE, HWND, PSID};
use windows::Win32::Security::{
    AllocateAndInitializeSid, CheckTokenMembership, FreeSid, SID_IDENTIFIER_AUTHORITY,
};
use windows::Win32::System::SystemServices::{
    DOMAIN_ALIAS_RID_ADMINS, SECURITY_BUILTIN_DOMAIN_RID,
};
use windows::Win32::UI::Shell::ShellExecuteW;

pub fn is_admin() -> bool {
    unsafe {
        // pub struct PSID(pub *mut core::ffi::c_void);
        let mut sid: PSID = PSID(ptr::null_mut());
        let nt_authority: SID_IDENTIFIER_AUTHORITY = SID_IDENTIFIER_AUTHORITY {
            Value: [0, 0, 0, 0, 0, 5], // SECURITY_NT_AUTHORITY
        };

        if AllocateAndInitializeSid(
            &nt_authority,
            2,
            SECURITY_BUILTIN_DOMAIN_RID as u32,
            DOMAIN_ALIAS_RID_ADMINS as u32,
            0,
            0,
            0,
            0,
            0,
            0,
            &mut sid,
        )
        .is_ok()
        {
            let mut is_member = BOOL(0);
            let result = CheckTokenMembership(HANDLE(0), sid, &mut is_member);
            FreeSid(sid);

            return result.is_ok() && is_member.as_bool();
        }
    }
    false
}

pub fn run_as_admin() -> bool {
    unsafe {
        let result = ShellExecuteW(
            HWND(0),
            PWSTR("runas".encode_utf16().collect::<Vec<u16>>().as_ptr() as _),
            PWSTR(
                std::env::current_exe()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .encode_utf16()
                    .collect::<Vec<u16>>()
                    .as_ptr() as _,
            ),
            PWSTR(ptr::null_mut()),
            PWSTR(ptr::null_mut()),
            windows::Win32::UI::WindowsAndMessaging::SW_SHOW,
        );

        !result.is_invalid()
    }
}
