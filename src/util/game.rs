#![allow(non_camel_case_types)]

use std::ffi::{c_void, OsStr, OsString};
use std::mem::{self};
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

use tokio::task;
use tokio::time::sleep;

use windows::{
    core::{Result, PWSTR},
    Wdk::{
        Foundation::{NtQueryObject, OBJECT_INFORMATION_CLASS, OBJECT_NAME_INFORMATION},
        System::Threading::ProcessHandleInformation,
    },
    Win32::{
        Foundation::{
            CloseHandle, DuplicateHandle, BOOL, DUPLICATE_CLOSE_SOURCE, DUPLICATE_SAME_ACCESS,
            HANDLE, INVALID_HANDLE_VALUE, NTSTATUS, STATUS_INFO_LENGTH_MISMATCH,
            STATUS_PIPE_DISCONNECTED, STATUS_PROCESS_IS_TERMINATING,
        },
        System::Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, Process32First, Process32Next,
            CREATE_TOOLHELP_SNAPSHOT_FLAGS, PROCESSENTRY32, TH32CS_SNAPPROCESS,
        },
        System::SystemServices::MAXIMUM_ALLOWED,
        System::Threading::{
            CreateProcessW, GetCurrentProcess, OpenProcess, TerminateProcess, CREATE_NEW_CONSOLE,
            CREATE_NO_WINDOW, PROCESS_ALL_ACCESS, PROCESS_CREATION_FLAGS, PROCESS_INFORMATION,
            STARTUPINFOW,
        },
    },
};

use super::custom_windows::{self, PROCESS_HANDLE_SNAPSHOT_INFORMATION};

/// see https://www.geoffchappell.com/studies/windows/km/ntoskrnl/api/ex/sysinfo/handle.htm
const OBJECT_NAME_INFORMATION: OBJECT_INFORMATION_CLASS = OBJECT_INFORMATION_CLASS(0x1);

pub struct GameManager {
    children: Arc<Mutex<Vec<(u32, HANDLE)>>>, // Store PID alongside HANDLE
}

impl GameManager {
    pub fn new() -> Self {
        GameManager {
            children: Arc::new(Mutex::new(Vec::new())),
        }
    }

    // Modify `launch_game` and other related methods to handle `HANDLE` directly
    pub async fn launch_game(&self, game_path: PathBuf) -> bool {
        let game_launch = task::spawn_blocking(move || {
            let args = vec!["-launch"];
            let pi = spawn_console_process(game_path.to_str().unwrap_or_default(), args);
            println!("Launched game with pid: {}", pi.dwProcessId);
            pi
        });

        match game_launch.await {
            Ok(pi) if pi.hProcess.0 != 0 => {
                let mut children = self.children.lock().await;
                children.push((pi.dwProcessId, HANDLE(pi.hProcess.0)));

                unsafe { modify_processes_once() }
                    .await
                    .expect("should close");

                true
            }
            _ => {
                eprintln!("Failed to launch game in background thread");
                false
            }
        }
    }

    pub async fn kill_a_game(&self, target_pid: u32) {
        let mut children = self.children.lock().await;
        if let Some(index) = children.iter().position(|(pid, _)| *pid == target_pid) {
            let (_, handle) = children[index];
            unsafe {
                let _ = TerminateProcess(handle, 0);
            }
            println!("Killing game with pid: {}", target_pid);
            children.remove(index);
        }
    }

    pub async fn kill_all_games(&self) {
        let mut children = self.children.lock().await;
        for (pid, handle) in children.iter() {
            println!("Killing game with pid: {}", pid);
            unsafe {
                let _ = TerminateProcess(*handle, 0);
            }
        }
        children.clear(); // Clear all children after killing them
    }
}

struct HandleWrapper(HANDLE);

impl HandleWrapper {
    fn new(dwflags: CREATE_TOOLHELP_SNAPSHOT_FLAGS) -> windows::core::Result<Self> {
        unsafe { CreateToolhelp32Snapshot(dwflags, 0).map(Self) }
    }

    fn get_handle(&self) -> HANDLE {
        self.0
    }
}

impl Drop for HandleWrapper {
    fn drop(&mut self) {
        unsafe {
            if !self.0.is_invalid() {
                let _ = CloseHandle(self.0);
            }
        }
    }
}

/// check and close
pub async unsafe fn modify_processes_once() -> Result<()> {
    let starcraft_name = b"StarCraft.exe\0"; // ANSI string with null termination
    let mut is_handle_found = false;

    while !is_handle_found {
        let h_snapshot = HandleWrapper::new(TH32CS_SNAPPROCESS)?;
        // Check if handle is invalid
        if h_snapshot.get_handle() == INVALID_HANDLE_VALUE {
            return Err(
                std::io::Error::new(std::io::ErrorKind::Other, "Invalid handle value").into(),
            );
        }
        let mut entry = PROCESSENTRY32::default();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32>() as u32;

        if Process32First(h_snapshot.get_handle(), &mut entry as *mut PROCESSENTRY32).is_ok() {
            'outer: loop {
                // Check for StarCraft.exe
                if unsafe {
                    std::ffi::CStr::from_ptr(entry.szExeFile.as_ptr()).to_bytes_with_nul()
                        == starcraft_name
                } {
                    let process_handle: HANDLE =
                        unsafe { OpenProcess(PROCESS_ALL_ACCESS, false, entry.th32ProcessID)? };

                    if process_handle == INVALID_HANDLE_VALUE {
                        println!("Failed to open process with ID: {}", entry.th32ProcessID);
                        continue; // skip to the next process
                    }

                    let mut buffer: Vec<u8> = Vec::new();
                    let mut dw_length: u32 = 0;

                    let mut status: NTSTATUS = custom_windows::ZwQueryInformationProcess(
                        process_handle,
                        ProcessHandleInformation,
                        std::ptr::null_mut(), // initially pass a null pointer
                        0,                    // and a length of 0
                        &mut dw_length,
                    );

                    while status == STATUS_INFO_LENGTH_MISMATCH {
                        buffer.resize(dw_length as usize, 0);

                        status = custom_windows::ZwQueryInformationProcess(
                            process_handle,
                            ProcessHandleInformation,
                            buffer.as_mut_ptr() as *mut c_void,
                            buffer.len() as u32,
                            &mut dw_length,
                        );
                    }

                    if status == STATUS_PROCESS_IS_TERMINATING {
                        continue;
                    }

                    if status.is_err() {
                        println!("Error: NTSTATUS({})", status.0);
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::Other,
                            "Failed to query process information",
                        )
                        .into());
                    }

                    // in cpp, pInformation
                    let handles_slice = unsafe {
                        let base_ptr =
                            buffer.as_ptr() as *const PROCESS_HANDLE_SNAPSHOT_INFORMATION;
                        // Calculate the pointer to the handles array. The offset should take into account the size of the struct.
                        let handles_ptr = (base_ptr.add(1) as *const u8)
                            .cast::<custom_windows::PROCESS_HANDLE_TABLE_ENTRY_INFO>(
                        );
                        // Create a slice from the handles array using the number of handles
                        std::slice::from_raw_parts(handles_ptr, (*base_ptr).NumberOfHandles)
                    };

                    for handle_info in handles_slice {
                        let handle = handle_info.HandleValue;

                        let mut copy_handle: HANDLE = HANDLE::default(); // zeroed()
                        let status = DuplicateHandle(
                            process_handle,
                            handle,
                            GetCurrentProcess(),
                            &mut copy_handle as *mut HANDLE,
                            MAXIMUM_ALLOWED,
                            false,
                            DUPLICATE_SAME_ACCESS,
                        );

                        if status.is_err() {
                            continue;
                        }

                        // 핸들정보 조회
                        let mut object_buf: Vec<u8> = Vec::new();
                        let mut dw_object_result: u32 = 0; // DWORD

                        let mut status = unsafe {
                            NtQueryObject(
                                copy_handle,
                                OBJECT_NAME_INFORMATION,
                                None,                        // pass None to get size
                                0,                           // Length is 0 for initial call
                                Some(&mut dw_object_result), // pointer to receive required size
                            )
                        };

                        while status == STATUS_INFO_LENGTH_MISMATCH {
                            object_buf.resize(dw_object_result as usize, 0);
                            status = unsafe {
                                NtQueryObject(
                                    copy_handle,
                                    OBJECT_NAME_INFORMATION, // Same class as before
                                    Some(object_buf.as_mut_ptr() as *mut c_void), // Now passing the actual buffer
                                    dw_object_result, // The length is now the correct size
                                    None,             // No need to pass size pointer again
                                )
                            };
                        }

                        if status == STATUS_PIPE_DISCONNECTED {
                            continue;
                        }

                        let _ = CloseHandle(copy_handle);

                        let p_object_info = unsafe {
                            // Transmute the buffer
                            (object_buf.as_ptr() as *const OBJECT_NAME_INFORMATION)
                                .as_ref()
                                .unwrap()
                        };

                        // Check if the Name length is non-zero
                        if p_object_info.Name.Length > 0 {
                            let name_slice = unsafe {
                                std::slice::from_raw_parts(
                                    p_object_info.Name.Buffer.as_ptr(), // Correctly access the raw pointer
                                    p_object_info.Name.Length as usize / 2, // Divide by 2 because it's likely u16 units
                                )
                            };

                            let name: OsString = OsStringExt::from_wide(name_slice);

                            // Searching for the specific substring
                            if name
                                .to_string_lossy()
                                .contains("Starcraft Check For Other Instances")
                            {
                                // let mut copy_handle: HANDLE = HANDLE(0); // Equivalent to nullptr in C++
                                let status = DuplicateHandle(
                                    process_handle,
                                    handle,
                                    GetCurrentProcess(),
                                    &mut copy_handle,
                                    MAXIMUM_ALLOWED,
                                    false,
                                    DUPLICATE_CLOSE_SOURCE,
                                );

                                // Check status if necessary and then close the handle
                                if status.is_ok() {
                                    println!("\t - Closed proc_handle for StarCraft.exe!");
                                    let _ = CloseHandle(copy_handle);
                                    is_handle_found = true;
                                    break 'outer;
                                }
                            }
                        }
                    }
                }

                if Process32Next(h_snapshot.get_handle(), &mut entry).is_err() {
                    // let _ = CloseHandle(h_snapshot.get_handle()); // auto drop
                    break;
                }
            }
        }
    }

    Ok(())
}

/// Continuously check and close
pub async unsafe fn modify_processes() -> Result<()> {
    let starcraft_name = b"StarCraft.exe\0"; // ANSI string with null termination

    // process_name.push(0); // null-terminate
    loop {
        sleep(Duration::from_millis(100)).await;

        let h_snapshot = HandleWrapper::new(TH32CS_SNAPPROCESS)?;
        // Check if handle is invalid
        if h_snapshot.get_handle() == INVALID_HANDLE_VALUE {
            return Err(
                std::io::Error::new(std::io::ErrorKind::Other, "Invalid handle value").into(),
            );
        }
        let mut entry = PROCESSENTRY32::default();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32>() as u32;

        if Process32First(h_snapshot.get_handle(), &mut entry as *mut PROCESSENTRY32).is_ok() {
            loop {
                // Check for StarCraft.exe
                if unsafe {
                    std::ffi::CStr::from_ptr(entry.szExeFile.as_ptr()).to_bytes_with_nul()
                        == starcraft_name
                } {
                    let process_handle: HANDLE =
                        unsafe { OpenProcess(PROCESS_ALL_ACCESS, false, entry.th32ProcessID)? };

                    if process_handle == INVALID_HANDLE_VALUE {
                        println!("Failed to open process with ID: {}", entry.th32ProcessID);
                        continue; // skip to the next process
                    }

                    let mut buffer: Vec<u8> = Vec::new();
                    let mut dw_length: u32 = 0;

                    let mut status: NTSTATUS = custom_windows::ZwQueryInformationProcess(
                        process_handle,
                        ProcessHandleInformation,
                        std::ptr::null_mut(), // initially pass a null pointer
                        0,                    // and a length of 0
                        &mut dw_length,
                    );

                    while status == STATUS_INFO_LENGTH_MISMATCH {
                        buffer.resize(dw_length as usize, 0);

                        status = custom_windows::ZwQueryInformationProcess(
                            process_handle,
                            ProcessHandleInformation,
                            buffer.as_mut_ptr() as *mut c_void,
                            buffer.len() as u32,
                            &mut dw_length,
                        );
                    }

                    if status == STATUS_PROCESS_IS_TERMINATING {
                        continue;
                    }

                    if status.is_err() {
                        println!("Error: NTSTATUS({})", status.0);
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::Other,
                            "Failed to query process information",
                        )
                        .into());
                    }

                    let handles_slice = unsafe {
                        let base_ptr =
                            buffer.as_ptr() as *const PROCESS_HANDLE_SNAPSHOT_INFORMATION;
                        // Calculate the pointer to the handles array. The offset should take into account the size of the struct.
                        let handles_ptr = (base_ptr.add(1) as *const u8)
                            .cast::<custom_windows::PROCESS_HANDLE_TABLE_ENTRY_INFO>(
                        );
                        // Create a slice from the handles array using the number of handles
                        std::slice::from_raw_parts(handles_ptr, (*base_ptr).NumberOfHandles)
                    };

                    for handle_info in handles_slice {
                        let handle = handle_info.HandleValue;

                        let mut copy_handle: HANDLE = HANDLE::default(); // zeroed()
                        let status = DuplicateHandle(
                            process_handle,
                            handle,
                            GetCurrentProcess(),
                            &mut copy_handle as *mut HANDLE,
                            MAXIMUM_ALLOWED,
                            false,
                            DUPLICATE_SAME_ACCESS,
                        );

                        if status.is_err() {
                            continue;
                        }

                        // 핸들정보 조회
                        let mut object_buf: Vec<u8> = Vec::new();
                        let mut dw_object_result: u32 = 0; // DWORD

                        let mut status = unsafe {
                            NtQueryObject(
                                copy_handle,
                                OBJECT_NAME_INFORMATION,
                                None,                        // pass None to get size
                                0,                           // Length is 0 for initial call
                                Some(&mut dw_object_result), // pointer to receive required size
                            )
                        };

                        while status == STATUS_INFO_LENGTH_MISMATCH {
                            object_buf.resize(dw_object_result as usize, 0);
                            status = unsafe {
                                NtQueryObject(
                                    copy_handle,
                                    OBJECT_NAME_INFORMATION, // Same class as before
                                    Some(object_buf.as_mut_ptr() as *mut c_void), // Now passing the actual buffer
                                    dw_object_result, // The length is now the correct size
                                    None,             // No need to pass size pointer again
                                )
                            };
                        }

                        if status == STATUS_PIPE_DISCONNECTED {
                            continue;
                        }

                        let _ = CloseHandle(copy_handle);

                        let p_object_info = unsafe {
                            // Transmute the buffer
                            (object_buf.as_ptr() as *const OBJECT_NAME_INFORMATION)
                                .as_ref()
                                .unwrap()
                        };

                        // Check if the Name length is non-zero
                        if p_object_info.Name.Length > 0 {
                            let name_slice = unsafe {
                                std::slice::from_raw_parts(
                                    p_object_info.Name.Buffer.as_ptr(), // Correctly access the raw pointer
                                    p_object_info.Name.Length as usize / 2, // Divide by 2 because it's likely u16 units
                                )
                            };

                            let name: OsString = OsStringExt::from_wide(name_slice);

                            // Searching for the specific substring
                            if name
                                .to_string_lossy()
                                .contains("Starcraft Check For Other Instances")
                            {
                                // let mut copy_handle: HANDLE = HANDLE(0); // Equivalent to nullptr in C++
                                let status = DuplicateHandle(
                                    process_handle,
                                    handle,
                                    GetCurrentProcess(),
                                    &mut copy_handle,
                                    MAXIMUM_ALLOWED,
                                    false,
                                    DUPLICATE_CLOSE_SOURCE,
                                );

                                // Check status if necessary and then close the handle
                                if status.is_ok() {
                                    println!("Closed");
                                    let _ = CloseHandle(copy_handle);
                                }
                            }
                        }
                    }
                }

                if Process32Next(h_snapshot.get_handle(), &mut entry).is_err() {
                    // let _ = CloseHandle(h_snapshot.get_handle()); // auto drop
                    break;
                }
            }
        }
    }
}

/// Function to spawn a console process with no handle inheritance.
pub fn spawn_console_process(application: &str, args: Vec<&str>) -> PROCESS_INFORMATION {
    // Create command line string
    let mut cmd: Vec<u16> = OsStr::new(application).encode_wide().collect::<Vec<_>>();
    for arg in args {
        cmd.push(' ' as u16);
        cmd.extend(OsStr::new(arg).encode_wide());
    }
    cmd.push(0); // Null-terminate the entire command line

    let mut process_info = PROCESS_INFORMATION::default();
    let mut startup_info = STARTUPINFOW::default();
    startup_info.cb = mem::size_of::<STARTUPINFOW>() as u32;

    unsafe {
        CreateProcessW(
            None,                    // No module name (use command line)
            PWSTR(cmd.as_mut_ptr()), // Command line
            None,                    // Process security attributes
            None,                    // Primary thread security attributes
            BOOL(0),                 // handle inheritance option, 0 = FALSE
            PROCESS_CREATION_FLAGS(CREATE_NEW_CONSOLE.0 | CREATE_NO_WINDOW.0), // CREATE_NO_WINDOW | CREATE_NO_INHERIT_HANDLES
            None, // Use parent's environment block
            None, // Use parent's starting directory
            &mut startup_info,
            &mut process_info,
        )
        .expect("Failed to create process");
    }

    process_info
}

// WINDOWS
