use sha2::{Digest, Sha256};
use std::{ffi::{CStr, OsString, CString}, os::windows::ffi::OsStringExt, path::{Path, PathBuf}, fs::File, io::{Read, Write}};

use pelite::resources::version_info::VersionInfo;
use windows::{
    core::HSTRING,
    Win32::{
        Foundation::{HWND, MAX_PATH, RECT},
        System::{
            Com::{CoCreateInstance, CLSCTX_INPROC_SERVER},
            Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, Process32First, Process32Next, PROCESSENTRY32,
                TH32CS_SNAPALL,
            },
            SystemInformation::GetSystemDirectoryW,
        },
        UI::{
            Shell::{
                FileOpenDialog, IFileOpenDialog, IShellItem, SHCreateItemFromParsingName,
                FOS_FILEMUSTEXIST, FOS_PICKFOLDERS, SIGDN_FILESYSPATH,
            },
            WindowsAndMessaging::{GetDesktopWindow, GetWindowRect, SetWindowPos, SWP_NOSIZE},
        },
    },
};

pub trait RECTExt {
    fn dimensions(&self) -> (i32, i32);
}

impl RECTExt for RECT {
    fn dimensions(&self) -> (i32, i32) {
        (self.right - self.left, self.bottom - self.top)
    }
}

pub fn center_window(window: HWND) -> Result<(), windows::core::Error> {
    let screen = unsafe { GetDesktopWindow() };
    let mut screen_rect = RECT::default();
    unsafe { GetWindowRect(screen, &mut screen_rect)? };
    let (screen_width, screen_height) = screen_rect.dimensions();

    let mut window_rect = RECT::default();
    unsafe { GetWindowRect(window, &mut window_rect)? };
    let (window_width, window_height) = window_rect.dimensions();

    let x = screen_rect.left + (screen_width - window_width) / 2;
    let y = screen_rect.top + (screen_height - window_height) / 2;
    unsafe { SetWindowPos(window, None, x, y, 0, 0, SWP_NOSIZE)? };

    Ok(())
}

pub fn read_pe_version_info<'a>(image: &'a [u8]) -> Option<VersionInfo<'a>> {
    pelite::PeFile::from_bytes(image)
        .ok()?
        .resources()
        .ok()?
        .version_info()
        .ok()
}

pub fn open_select_folder_dialog<P: AsRef<Path>>(
    owner: HWND,
    default_folder: Option<P>,
) -> Option<PathBuf> {
    let dialog: IFileOpenDialog =
        unsafe { CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()? };

    unsafe {
        dialog.SetTitle(&HSTRING::from("Select a folder")).ok()?;
        dialog
            .SetOptions(FOS_FILEMUSTEXIST | FOS_PICKFOLDERS)
            .ok()?;

        if let Some(path) = default_folder {
            let default_folder_item: IShellItem =
                SHCreateItemFromParsingName(&HSTRING::from(path.as_ref().to_str().unwrap()), None)
                    .ok()?;
            dialog.SetDefaultFolder(&default_folder_item).ok()?;
        }

        dialog.Show(owner).ok()?
    }

    let result = unsafe { dialog.GetResult().ok()? };
    let path = unsafe { result.GetDisplayName(SIGDN_FILESYSPATH).ok()? };
    let path_str = unsafe { path.to_string().unwrap() };
    Some(path_str.into())
}

pub fn is_game_running() -> bool {
    let Ok(snapshot) = (unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPALL, 0) }) else {
        return false;
    };
    let mut entry = PROCESSENTRY32::default();
    entry.dwSize = std::mem::size_of::<PROCESSENTRY32>() as u32;
    let mut res = unsafe { Process32First(snapshot, &mut entry) };

    while res.is_ok() {
        let process_name = unsafe { CStr::from_ptr(entry.szExeFile.as_ptr()) };
        if process_name == c"umamusume.exe" || process_name == c"UmamusumePrettyDerby_Jpn.exe" {
            return true;
        }

        res = unsafe { Process32Next(snapshot, &mut entry) };
    }

    false
}

pub fn is_specific_process_running(exe_name: &str) -> bool {
    let Ok(exe_name_cstr) = CString::new(exe_name) else {
        return false;
    };

    let Ok(snapshot) = (unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPALL, 0) }) else {
        return false;
    };
    let mut entry = PROCESSENTRY32::default();
    entry.dwSize = std::mem::size_of::<PROCESSENTRY32>() as u32;
    let mut res = unsafe { Process32First(snapshot, &mut entry) };

    while res.is_ok() {
        let process_name = unsafe { CStr::from_ptr(entry.szExeFile.as_ptr()) };
        if process_name == exe_name_cstr.as_c_str() {
            return true;
        }
        res = unsafe { Process32Next(snapshot, &mut entry) };
    }

    false
}

pub fn get_system_directory() -> PathBuf {
    let mut buffer = [0u16; MAX_PATH as usize];
    let length = unsafe { GetSystemDirectoryW(Some(&mut buffer)) };
    PathBuf::from(OsString::from_wide(&buffer[0..length as usize]))
}

pub fn verify_file_hash(path: &Path, expected_hash: &str) -> Result<(), String> {
    let mut file = match File::open(path) {
        Ok(f) => f,
        Err(e) => return Err(format!("Could not open file: {}", e)),
    };

    let mut hasher = Sha256::new();
    let mut buffer = [0; 1024];

    loop {
        let n = match file.read(&mut buffer) {
            Ok(n) => n,
            Err(e) => return Err(format!("Could not read file: {}", e)),
        };
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }

    let found_hash = format!("{:x}", hasher.finalize());

    if found_hash.to_lowercase() == expected_hash.to_lowercase() {
        Ok(())
    } else {
        Err(format!(
            "Hash mismatch. Expected {}, but found {}",
            expected_hash, found_hash
        ))
    }
}

pub fn apply_patch(
    original_data: &[u8],
    patch_data: &[u8],
    output_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut new_exe_data = Vec::new();
    bsdiff::patch(original_data, &mut std::io::Cursor::new(patch_data), &mut new_exe_data)?;

    let mut temp_exe_file = File::create(output_path)?;
    temp_exe_file.write_all(&new_exe_data)?;

    Ok(())
}