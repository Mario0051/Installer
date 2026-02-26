use std::{fs::File, io::{Write, Read}, path::{Path, PathBuf}};

use pelite::resources::version_info::Language;
use registry::Hive;
use steamlocate::SteamDir;
use tinyjson::JsonValue;
use crate::i18n::t;
use windows::{core::HSTRING, Win32::{Foundation::HWND, UI::{Shell::{FOLDERID_RoamingAppData, SHGetKnownFolderPath, KF_FLAG_DEFAULT}, WindowsAndMessaging::{MessageBoxW, IDOK, IDYES, IDCANCEL, MB_ICONINFORMATION, MB_ICONWARNING, MB_ICONQUESTION, MB_OK, MB_OKCANCEL, MB_YESNO, MB_RETRYCANCEL}}}};

use crate::utils::{self, get_system_directory};

const LAUNCH_OPT_BACKUP_FILE: &str = ".hachimi_launch_options.bak";

#[derive(Clone, Copy, Eq, PartialEq)]
pub enum GameVersion {
    DMM,
    Steam,
    SteamGlobal
}

pub struct Installer {
    dmm_install_dir: Option<PathBuf>,
    steam_install_dir: Option<PathBuf>,
    steam_global_install_dir: Option<PathBuf>,
    install_dir: Option<PathBuf>,
    game_version: Option<GameVersion>,

    pub target: Target,
    pub custom_target: Option<String>,
    system_dir: PathBuf,
    pub hwnd: Option<HWND>
}

impl Installer {
    fn detect_version_from_dir(dir: &Path) -> Option<GameVersion> {
        if dir.join("umamusume.exe").is_file() {
            Some(GameVersion::DMM)
        } else if dir.join("UmamusumePrettyDerby_Jpn.exe").is_file() {
            Some(GameVersion::Steam)
        } else if dir.join("UmamusumePrettyDerby.exe").is_file() {
            Some(GameVersion::SteamGlobal)
        } else {
            None
        }
    }

    pub fn new(target: Target, custom_target: Option<String>) -> Installer {
        Installer {
            dmm_install_dir: None,
            steam_install_dir: None,
            steam_global_install_dir: None,
            install_dir: None,
            game_version: None,
            target,
            custom_target,
            system_dir: get_system_directory(),
            hwnd: None
        }
    }

    pub fn set_install_dir(&mut self, dir: PathBuf) -> Result<(), Error> {
        match Self::detect_version_from_dir(&dir) {
            Some(version) => {
                self.install_dir = Some(dir.clone());
                self.game_version = Some(version);
                match version {
                    GameVersion::DMM => self.dmm_install_dir = Some(dir),
                    GameVersion::Steam => self.steam_install_dir = Some(dir),
                    GameVersion::SteamGlobal => self.steam_global_install_dir = Some(dir),
                }
                Ok(())
            }
            None => Err(Error::InvalidInstallDir)
        }
    }

    pub fn install_dir(&self) -> Option<&PathBuf> {
        self.install_dir.as_ref()
    }

    pub fn game_version(&self) -> Option<GameVersion> {
        self.game_version
    }

    pub fn detect_install_dir(&mut self) {
        if let Some(dmm_dir) = Self::detect_dmm_install_dir() {
            self.install_dir = Some(dmm_dir);
            self.game_version = Some(GameVersion::DMM);
        } else if let Some(steam_dir) = Self::detect_steam_install_dir() {
            self.install_dir = Some(steam_dir);
            self.game_version = Some(GameVersion::Steam);
        } else if let Some(steam_global_dir) = Self::detect_steam_global_install_dir() {
            self.install_dir = Some(steam_global_dir);
            self.game_version = Some(GameVersion::SteamGlobal);
        }
    }

    pub fn detect_install_dirs(&mut self) {
        self.dmm_install_dir = Self::detect_dmm_install_dir();
        self.steam_install_dir = Self::detect_steam_install_dir();
        self.steam_global_install_dir = Self::detect_steam_global_install_dir();

        if self.install_dir.is_none() {
            if self.dmm_install_dir.is_some() {
                self.set_game_version(GameVersion::DMM);
            } else if self.steam_install_dir.is_some() {
                self.set_game_version(GameVersion::Steam);
            } else if self.steam_global_install_dir.is_some() {
                self.set_game_version(GameVersion::SteamGlobal);
            }
        }
    }

    pub fn dmm_install_dir(&self) -> Option<&PathBuf> {
        self.dmm_install_dir.as_ref()
    }

    pub fn steam_install_dir(&self) -> Option<&PathBuf> {
        self.steam_install_dir.as_ref()
    }

    pub fn steam_global_install_dir(&self) -> Option<&PathBuf> {
        self.steam_global_install_dir.as_ref()
    }

    pub fn set_game_version(&mut self, version: GameVersion) -> Option<&PathBuf> {
        self.game_version = Some(version);
        match version {
            GameVersion::DMM => self.install_dir = self.dmm_install_dir.clone(),
            GameVersion::Steam => self.install_dir = self.steam_install_dir.clone(),
            GameVersion::SteamGlobal => self.install_dir = self.steam_global_install_dir.clone(),
        }
        self.install_dir.as_ref()
    }

    fn detect_dmm_install_dir() -> Option<PathBuf> {
        let app_data_dir_wstr = unsafe { SHGetKnownFolderPath(&FOLDERID_RoamingAppData, KF_FLAG_DEFAULT, None).ok()? };
        let app_data_dir_str = unsafe { app_data_dir_wstr.to_string().ok()? };
        let app_data_dir = Path::new(&app_data_dir_str);
        let mut dmm_config_path = app_data_dir.join("dmmgameplayer5");
        dmm_config_path.push("dmmgame.cnf");

        let config_str = std::fs::read_to_string(dmm_config_path).ok()?;
        let JsonValue::Object(config) = config_str.parse().ok()? else {
            return None;
        };
        let JsonValue::Array(config_contents) = &config["contents"] else {
            return None;
        };
        for value in config_contents {
            let JsonValue::Object(game) = value else {
                return None;
            };

            let JsonValue::String(product_id) = &game["productId"] else {
                continue;
            };
            if product_id != "umamusume" {
                continue;
            }

            let JsonValue::Object(detail) = &game["detail"] else {
                return None;
            };
            let JsonValue::String(path_str) = &detail["path"] else {
                return None;
            };

            let path = PathBuf::from(path_str);
            return if path.is_dir() {
                Some(path)
            }
            else {
                None
            }
        }

        None
    }

    fn detect_steam_install_dir() -> Option<PathBuf> {
        const STEAM_APP_ID: u32 = 3564400;
        const GAME_EXE_NAME: &str = "UmamusumePrettyDerby_Jpn.exe";

        if let Ok(steamdir) = SteamDir::locate() {
            if let Ok(Some((app, library))) = steamdir.find_app(STEAM_APP_ID) {

                let game_path = library.path()
                    .join("steamapps")
                    .join("common")
                    .join(&app.install_dir);

                if game_path.join(GAME_EXE_NAME).is_file() {
                    return Some(game_path);
                }
            }
        }

        None
    }

    fn detect_steam_global_install_dir() -> Option<PathBuf> {
        const STEAM_APP_ID: u32 = 3224770;
        const GAME_EXE_NAME: &str = "UmamusumePrettyDerby.exe";

        if let Ok(steamdir) = SteamDir::locate() {
            if let Ok(Some((app, library))) = steamdir.find_app(STEAM_APP_ID) {

                let game_path = library.path()
                    .join("steamapps")
                    .join("common")
                    .join(&app.install_dir);

                if game_path.join(GAME_EXE_NAME).is_file() {
                    return Some(game_path);
                }
            }
        }

        None
    }

    fn get_install_method(&self, target: Target) -> InstallMethod {
        match target {
            Target::UnityPlayer => InstallMethod::DotLocal,
            Target::CriManaVpx => {
                if self.game_version == Some(GameVersion::Steam) || self.game_version == Some(GameVersion::SteamGlobal) {
                    InstallMethod::Direct
                } else {
                    InstallMethod::PluginShim
                }
            }
        }
    }

    fn get_target_path_internal(&self, target: Target, p: impl AsRef<Path>) -> Option<PathBuf> {
        let install_dir = self.install_dir.as_ref()?;
        Some(match self.get_install_method(target) {
            InstallMethod::DotLocal => {
                let exe_name = match self.game_version {
                    Some(GameVersion::Steam) => "UmamusumePrettyDerby_Jpn.exe",
                    Some(GameVersion::SteamGlobal) => "UmamusumePrettyDerby.exe",
                    Some(GameVersion::DMM) | _ => "umamusume.exe",
                };
                let local_folder_name = format!("{}.local", exe_name);
                install_dir.join(local_folder_name).join(p)
            }
            InstallMethod::PluginShim => self.system_dir.join(p),
            InstallMethod::Direct => install_dir.join(p),
        })
    }

    pub fn get_target_path(&self, target: Target) -> Option<PathBuf> {
        self.get_target_path_internal(target, target.dll_name())
    }

    pub fn get_current_target_path(&self) -> Option<PathBuf> {
        self.get_target_path_internal(self.target, if let Some(custom_target) = &self.custom_target {
            custom_target
        }
        else {
            self.target.dll_name()
        })
    }

    const LANG_NEUTRAL_UNICODE: Language = Language { lang_id: 0x0000, charset_id: 0x04b0 };
    pub fn get_target_version_info(&self, target: Target) -> Option<TargetVersionInfo> {
        let path = self.get_target_path(target)?;
        let map = pelite::FileMap::open(&path).ok()?;

        // File exists, so return empty version info if we can't read it
        let Some(version_info) = utils::read_pe_version_info(map.as_ref()) else {
            return Some(TargetVersionInfo::default());
        };

        Some(TargetVersionInfo {
            name: version_info.value(Self::LANG_NEUTRAL_UNICODE, "ProductName"),
            version: version_info.value(Self::LANG_NEUTRAL_UNICODE, "ProductVersion")
        })
    }

    pub fn get_target_display_label(&self, target: Target) -> String {
        if let Some(version_info) = self.get_target_version_info(target) {
            version_info.get_display_label(target)
        }
        else {
            target.dll_name().to_owned()
        }
    }

    pub fn is_current_target_installed(&self) -> bool {
        let Some(path) = self.get_current_target_path() else {
            return false;
        };
        let Ok(metadata) = std::fs::metadata(&path) else {
            return false;
        };
        metadata.is_file()
    }

    pub fn get_hachimi_installed_target(&self) -> Option<Target> {
        for target in Target::VALUES {
            if let Some(version_info) = self.get_target_version_info(*target) {
                if version_info.is_hachimi() {
                    return Some(*target);
                }
            }
        }
        None
    }

    pub fn pre_install(&self) -> Result<(), Error> {
        if self.get_install_method(self.target) == InstallMethod::PluginShim {
            let dest_dll = self.get_dest_plugin_path().ok_or(Error::NoInstallDir)?;
            let src_dll = self.get_src_plugin_path().ok_or(Error::NoInstallDir)?;

            if !dest_dll.exists() && !src_dll.exists() {
                return Err(Error::CannotFindTarget);
            }
        }
        Ok(())
    }

    fn ensure_steam_closed(&self) -> Result<(), Error> {
        if self.hwnd.is_none() { return Ok(()); }

        while utils::is_specific_process_running("steam.exe") {
            let res = unsafe {
                MessageBoxW(
                    self.hwnd.as_ref(),
                    &HSTRING::from(t!("installer.steam_running_prompt")),
                    &HSTRING::from(t!("installer.warning")),
                    MB_RETRYCANCEL | MB_ICONWARNING
                )
            };

            if res == IDCANCEL {
                return Err(Error::Generic("Steam is running. Cannot modify configuration.".into()));
            }
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
        Ok(())
    }

    pub fn install(&self) -> Result<(), Error> {
        let initial_dll_path = self.get_current_target_path().ok_or(Error::NoInstallDir)?;

        std::fs::create_dir_all(initial_dll_path.parent().unwrap())?;
        let mut file = File::create(&initial_dll_path)?;

        #[cfg(feature = "compress_dll")]
        file.write(&include_bytes_zstd!("hachimi.dll", 19))?;

        #[cfg(not(feature = "compress_dll"))]
        file.write(include_bytes!("../hachimi.dll"))?;

        let install_path = self.install_dir.as_ref().ok_or(Error::NoInstallDir)?;

        const EXPECTED_ORIGINAL_HASH: &str = "6519de9bbae11d3f7b779ce09b74e0a0c408b814518bff93da295c8f7b65ad5a";

        match self.game_version {
            Some(GameVersion::DMM) => {},
            Some(GameVersion::SteamGlobal) => {},
            Some(GameVersion::Steam) => {
                let steam_exe_path = install_path.join("UmamusumePrettyDerby_Jpn.exe");
                let patched_exe_path = install_path.join("FunnyHoney.exe");

                if let Err(e) = utils::verify_file_hash(&steam_exe_path, EXPECTED_ORIGINAL_HASH) {
                    let error_msg = t!(
                        "installer.error_verification_body",
                        file_name = "UmamusumePrettyDerby_Jpn.exe",
                        details = e.to_string()
                    );
                    return Err(Error::VerificationError(error_msg));
                }

                let original_exe_data = std::fs::read(&steam_exe_path)?;
                let compressed_patch_data = include_bytes!("../umamusume.patch.zst");
                let mut patch_data = Vec::new();
                let mut decoder = zstd::Decoder::new(&compressed_patch_data[..])?;
                decoder.read_to_end(&mut patch_data)?;

                utils::apply_patch(&original_exe_data, &patch_data, &patched_exe_path)
                    .map_err(|e| Error::Generic(e.to_string().into()))?;

                let launcher_bytes = include_bytes!(concat!(env!("OUT_DIR"), "/hachimi_launcher.exe"));
                let launcher_path = install_path.join("hachimi_launcher.exe");
                std::fs::write(&launcher_path, launcher_bytes)?;

                if let Err(e) = self.setup_launch_options("3564400") {
                    return Err(e);
                }
            },
            None => {
                return Err(Error::IoError(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "Could not find a valid game executable."
                )));
            }
        }

        Ok(())
    }

    fn escape_vdf_value(val: &str) -> String {
        val.replace('\\', "\\\\").replace('"', "\\\"")
    }

    fn find_vdf_app_range(content: &str, app_id: &str) -> Option<(usize, usize)> {
        let app_key = format!("\"{}\"", app_id);
        let app_idx = content.find(&app_key)?;

        let open_brace_rel = content[app_idx..].find('{')?;
        let start_block = app_idx + open_brace_rel + 1;

        let mut depth = 1;
        for (i, c) in content[start_block..].char_indices() {
            match c {
                '{' => depth += 1,
                '}' => depth -= 1,
                _ => {}
            }
            if depth == 0 {
                return Some((start_block, start_block + i));
            }
        }
        None
    }

    fn find_vdf_value_range(text: &str) -> Option<(usize, usize)> {
        let chars: Vec<(usize, char)> = text.char_indices().collect();
        let mut start_quote = None;
        let mut idx = 0;

        while idx < chars.len() {
            let (pos, c) = chars[idx];
            if !c.is_whitespace() {
                if c == '"' {
                    start_quote = Some(pos);
                    idx += 1;
                    break;
                } else {
                    return None;
                }
            }
            idx += 1;
        }

        let start_pos = start_quote?;

        while idx < chars.len() {
            let (pos, c) = chars[idx];
            if c == '\\' {
                idx += 2;
                continue;
            }
            if c == '"' {
                return Some((start_pos, pos));
            }
            idx += 1;
        }
        None
    }

    fn get_launch_command(&self) -> Result<String, Error> {
        if std::env::var("WINEPREFIX").is_ok() || std::env::var("WINEDIR").is_ok() {
            Ok(String::from("cp -f FunnyHoney.exe UmamusumePrettyDerby_Jpn.exe && %command%"))
        }
        else {
            let install_path = self.install_dir.as_ref().ok_or(Error::NoInstallDir)?;
            let launcher_path = install_path.join("hachimi_launcher.exe");

            Ok(format!("\"{}\" %command%", launcher_path.display()))
        }
    }

    fn setup_launch_options(&self, app_id: &str) -> Result<(), Error> {
        let raw_launch_cmd = self.get_launch_command()?;
        let escaped_val = Self::escape_vdf_value(&raw_launch_cmd);
        let expected_vdf_value = format!("\"{}\"", escaped_val);

        let steam_dir = SteamDir::locate().map_err(|_| Error::Generic("Could not locate Steam".into()))?;
        let userdata_dir = steam_dir.path().join("userdata");

        if userdata_dir.exists() {
            for entry in std::fs::read_dir(&userdata_dir)? {
                let entry = entry?;
                let config_path = entry.path().join("config").join("localconfig.vdf");

                if config_path.exists() {
                    let content = std::fs::read_to_string(&config_path)?;

                    if let Some((start_block, end_block)) = Self::find_vdf_app_range(&content, app_id) {
                        let block_slice = &content[start_block..end_block];

                        if let Some(rel_key_idx) = block_slice.find("\"LaunchOptions\"") {
                            let abs_key_idx = start_block + rel_key_idx;
                            let after_key_idx = abs_key_idx + "\"LaunchOptions\"".len();
                            let search_area = &content[after_key_idx..end_block];

                            if let Some((sq, eq)) = Self::find_vdf_value_range(search_area) {
                                let val_start_abs = after_key_idx + sq;
                                let val_end_abs = after_key_idx + eq + 1;
                                let current_val = &content[val_start_abs..val_end_abs];

                                if current_val == expected_vdf_value {
                                    return Ok(());
                                }
                            }
                        }
                    }
                }
            }
        }

        if self.hwnd.is_some() {
            let res = unsafe {
                MessageBoxW(
                    self.hwnd.as_ref(),
                    &HSTRING::from(t!("installer.setup_launch_options_prompt")),
                    &HSTRING::from(t!("installer.setup_launch_options_title")),
                    MB_ICONQUESTION | MB_YESNO
                )
            };
            if res != IDYES { return Ok(()); }
        }

        self.ensure_steam_closed()?;

        if !userdata_dir.exists() { return Ok(()); }

        let install_path = self.install_dir.as_ref().ok_or(Error::NoInstallDir)?;
        let backup_path = install_path.join(LAUNCH_OPT_BACKUP_FILE);

        for entry in std::fs::read_dir(userdata_dir)? {
            let entry = entry?;
            let config_path = entry.path().join("config").join("localconfig.vdf");

            if config_path.exists() {
                let mut content = std::fs::read_to_string(&config_path)?;
                let mut modified = false;
                let mut backup_value = String::new();

                if let Some((start_block, end_block)) = Self::find_vdf_app_range(&content, app_id) {
                    let block_slice = &content[start_block..end_block];

                    if let Some(rel_key_idx) = block_slice.find("\"LaunchOptions\"") {
                        let abs_key_idx = start_block + rel_key_idx;
                        let after_key_idx = abs_key_idx + "\"LaunchOptions\"".len();
                        let search_area = &content[after_key_idx..end_block];

                        if let Some((sq, eq)) = Self::find_vdf_value_range(search_area) {
                             let val_start_abs = after_key_idx + sq + 1;
                             let val_end_abs   = after_key_idx + eq;

                             backup_value = content[val_start_abs..val_end_abs].to_string();

                             let range_to_replace = (after_key_idx + sq)..(after_key_idx + eq + 1);
                             content.replace_range(range_to_replace, &expected_vdf_value);
                             modified = true;
                        }
                    } else {
                        backup_value = String::new();

                        let insert_str = format!("\t\"LaunchOptions\"\t\t\"{}\"\n\t\t\t\t\t", escaped_val);
                        content.insert_str(end_block, &insert_str);
                        modified = true;
                    }
                }

                if modified {
                    std::fs::write(&backup_path, &backup_value)?;
                    std::fs::write(&config_path, content)?;
                    break;
                }
            }
        }
        Ok(())
    }

    fn restore_launch_options(&self, app_id: &str) -> Result<(), Error> {
        if self.hwnd.is_some() {
            let res = unsafe {
                MessageBoxW(
                    self.hwnd.as_ref(),
                    &HSTRING::from(t!("installer.restore_launch_options_prompt")),
                    &HSTRING::from(t!("installer.restore_launch_options_title")),
                    MB_ICONQUESTION | MB_YESNO
                )
            };
            if res != IDYES { return Ok(()); }
        }

        self.ensure_steam_closed()?;

        let install_path = self.install_dir.as_ref().ok_or(Error::NoInstallDir)?;
        let backup_path = install_path.join(LAUNCH_OPT_BACKUP_FILE);
        
        if !backup_path.exists() { return Ok(()); }

        let backup_value = std::fs::read_to_string(&backup_path).unwrap_or_default();

        let steam_dir = SteamDir::locate().map_err(|_| Error::Generic("Could not locate Steam".into()))?;
        let userdata_dir = steam_dir.path().join("userdata");
        if !userdata_dir.exists() { return Ok(()); }

        for entry in std::fs::read_dir(userdata_dir)? {
            let entry = entry?;
            let config_path = entry.path().join("config").join("localconfig.vdf");

            if config_path.exists() {
                let mut content = std::fs::read_to_string(&config_path)?;
                let mut modified = false;

                if let Some((start_block, end_block)) = Self::find_vdf_app_range(&content, app_id) {
                    let block_slice = &content[start_block..end_block];

                    if let Some(rel_key_idx) = block_slice.find("\"LaunchOptions\"") {
                        let abs_key_idx = start_block + rel_key_idx;
                        let after_key_idx = abs_key_idx + "\"LaunchOptions\"".len();
                        let search_area = &content[after_key_idx..end_block];

                        if let Some((sq, eq)) = Self::find_vdf_value_range(search_area) {
                             let val_start_abs = after_key_idx + sq + 1;
                             let val_end_abs   = after_key_idx + eq;
                             let current_val = &content[val_start_abs..val_end_abs];

                             if current_val.contains("FunnyHoney.exe") {
                                 let range_to_replace = (after_key_idx + sq)..(after_key_idx + eq + 1);
                                 let restored_val = format!("\"{}\"", backup_value);
                                 content.replace_range(range_to_replace, &restored_val);
                                 modified = true;
                             }
                        }
                    }
                }

                if modified {
                    std::fs::write(&config_path, content)?;
                }
            }
        }

        _ = std::fs::remove_file(backup_path);

        Ok(())
    }

    pub fn post_install(&self) -> Result<(), Error> {
        match self.get_install_method(self.target) {
            InstallMethod::DotLocal => {
                // Install Cellar
                let main_dll_path = self.get_current_target_path().ok_or(Error::NoInstallDir)?;
                let parent_dir = main_dll_path.parent().unwrap();

                let path = parent_dir.join("apphelp.dll");
                std::fs::create_dir_all(path.parent().unwrap())?;
                let mut file = File::create(&path)?;

                #[cfg(feature = "compress_dll")]
                file.write(&include_bytes_zstd!("cellar.dll", 19))?;

                #[cfg(not(feature = "compress_dll"))]
                file.write(include_bytes!("../cellar.dll"))?;

                // Check for DLL redirection
                match Hive::LocalMachine.open(
                    r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File Execution Options",
                    registry::Security::Read | registry::Security::SetValue
                ) {
                    Ok(regkey) => {
                        if regkey.value("DevOverrideEnable")
                            .ok()
                            .map(|v| match v {
                                registry::Data::U32(v) => v,
                                _ => 0
                            })
                            .unwrap_or(0) == 0
                        {
                            let res = unsafe {
                                MessageBoxW(
                                    self.hwnd.as_ref(),
                                    &HSTRING::from(t!("installer.dotlocal_not_enabled")),
                                    &HSTRING::from(t!("installer.install")),
                                    MB_ICONINFORMATION | MB_OKCANCEL
                                )
                            };
                            if res == IDOK {
                                regkey.set_value("DevOverrideEnable", &registry::Data::U32(1))?;
                                unsafe {
                                    MessageBoxW(
                                        self.hwnd.as_ref(),
                                        &HSTRING::from(t!("installer.restart_to_apply")),
                                        &HSTRING::from(t!("installer.dll_redirection_enabled")),
                                        MB_ICONINFORMATION | MB_OK
                                    );
                                }
                            }
                        }
                    },
                    Err(e) => {
                        unsafe { MessageBoxW(
                            self.hwnd.as_ref(),
                            &HSTRING::from(t!("installer.failed_open_ifeo", error = e)),
                            &HSTRING::from(t!("installer.warning")),
                            MB_OK | MB_ICONWARNING
                        )};
                    }
                }
            },
            InstallMethod::PluginShim => {
                let dest_dll = self.get_dest_plugin_path().ok_or(Error::NoInstallDir)?;
                let src_dll = self.get_src_plugin_path().ok_or(Error::NoInstallDir)?;

                if src_dll.exists() {
                    std::fs::create_dir_all(dest_dll.parent().unwrap())?;
                    std::fs::copy(&src_dll, &dest_dll)?;
                    std::fs::remove_file(&src_dll)?;
                }
            },
            InstallMethod::Direct => {}
        }
        Ok(())
    }

    pub fn uninstall(&self) -> Result<(), Error> {
        let path = self.get_current_target_path().ok_or(Error::NoInstallDir)?;
        std::fs::remove_file(&path)?;

        match self.get_install_method(self.target) {
            InstallMethod::DotLocal => {
                let parent = path.parent().unwrap();
                // Also delete Cellar
                _ = std::fs::remove_file(parent.join("apphelp.dll"));
                // Only remove if its empty
                _ = std::fs::remove_dir(parent);
            },
            InstallMethod::PluginShim => {
                let dest_dll = self.get_dest_plugin_path().ok_or(Error::NoInstallDir)?;
                let src_dll = self.get_src_plugin_path().ok_or(Error::NoInstallDir)?;
                if !src_dll.exists() {
                    std::fs::copy(&dest_dll, &src_dll)?;
                    std::fs::remove_file(&dest_dll)?;
                }
            },
            InstallMethod::Direct => {}
        }

        if self.game_version == Some(GameVersion::Steam) {
            let install_path = self.install_dir.as_ref().ok_or(Error::NoInstallDir)?;

            let patched_path = install_path.join("FunnyHoney.exe");
            if patched_path.exists() { std::fs::remove_file(patched_path)?; }

            let launcher_path = install_path.join("hachimi_launcher.exe");
            if launcher_path.exists() { std::fs::remove_file(launcher_path)?; }

            self.restore_launch_options("3564400")?;
        }

        Ok(())
    }

    pub fn get_dest_plugin_path(&self) -> Option<PathBuf> {
        Some(self.install_dir.as_ref()?.join(format!("hachimi\\{}", self.target.dll_name())))
    }

    pub fn get_src_plugin_path(&self) -> Option<PathBuf> {
        Some(self.install_dir.as_ref()?.join(format!("umamusume_Data\\Plugins\\x86_64\\{}", self.target.dll_name())))
    }
}

impl Default for Installer {
    fn default() -> Installer {
        let mut installer = Self::new(Target::default(), None);
        installer.detect_install_dirs();
        installer
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub enum Target {
    UnityPlayer,
    CriManaVpx
}

impl Target {
    pub const VALUES: &[Self] = &[
        Self::UnityPlayer,
        Self::CriManaVpx
    ];

    pub fn dll_name(&self) -> &'static str {
        match self {
            Self::UnityPlayer => "UnityPlayer.dll",
            Self::CriManaVpx => "cri_mana_vpx.dll"
        }
    }
}

impl Default for Target {
    fn default() -> Self {
        Self::UnityPlayer
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum InstallMethod {
    DotLocal,
    PluginShim,
    Direct,
}

#[derive(Debug, Default)]
pub struct TargetVersionInfo {
    pub name: Option<String>,
    pub version: Option<String>
}

impl TargetVersionInfo {
    pub fn get_display_label(&self, target: Target) -> String {
        let name = self.name.clone().unwrap_or_else(|| "Unknown".to_string());
        format!("* {} ({})", target.dll_name(), name)
    }

    pub fn is_hachimi(&self) -> bool {
        if let Some(name) = &self.name {
            return name == "Hachimi";
        }
        false
    }
}

#[derive(Debug)]
pub enum Error {
    NoInstallDir,
    InvalidInstallDir,
    CannotFindTarget,
    IoError(std::io::Error),
    RegistryValueError(registry::value::Error),
    VerificationError(String),
    Generic(Box<dyn std::error::Error + Send + Sync>),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::NoInstallDir => write!(f, "{}", t!("error.no_install_dir")),
            Error::InvalidInstallDir => write!(f, "{}", t!("error.invalid_install_dir")),
            Error::CannotFindTarget => write!(f, "{}", t!("error.cannot_find_target")),
            Error::IoError(e) => write!(f, "{}", t!("error.io_error", error = e)),
            Error::RegistryValueError(e) => write!(f, "{}", t!("error.registry_value_error", error = e)),
            Error::VerificationError(e) => write!(f, "{}", t!("error.verification_error", error = e)),
            Error::Generic(e) => write!(f, "{}", t!("error.generic", error = e)),
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::IoError(e)
    }
}

impl From<registry::value::Error> for Error {
    fn from(e: registry::value::Error) -> Self {
        Error::RegistryValueError(e)
    }
}
