use std::{fs, path::Path};
use windows::{
    core::{Interface, HSTRING},
    Win32::{
        Storage::EnhancedStorage::PKEY_AppUserModel_ID,
        System::Com::{
            CoCreateInstance, CoTaskMemFree, IPersistFile, StructuredStorage::PROPVARIANT,
            CLSCTX_INPROC_SERVER, STGM_READ,
        },
        UI::Shell::{
            FOLDERID_Programs, IShellLinkW, PropertiesSystem::IPropertyStore, SHGetKnownFolderPath,
            ShellLink, KF_FLAG_DEFAULT,
        },
    },
};

// The shell discovers desktop notification senders through a Start Menu shortcut.
// Its AUMID must be the same as the notifier and installer, including in development.
pub(super) fn register(executable: &Path, app_id: &str) -> Result<(), String> {
    let result = (|| -> windows::core::Result<()> {
        let programs = unsafe { SHGetKnownFolderPath(&FOLDERID_Programs, KF_FLAG_DEFAULT, None)? };
        let path = unsafe { programs.to_string() };
        unsafe { CoTaskMemFree(Some(programs.0.cast())) };
        let name = if app_id == super::APP_ID {
            "Jarvis"
        } else {
            "Jarvis Dev"
        };
        let shortcut = Path::new(&path?).join(name).join(format!("{name}.lnk"));
        ensure(&shortcut, executable, app_id)
    })();
    result.map_err(|error| super::native_error("registrar o atalho do Jarvis", error.code().0))
}

fn ensure(shortcut: &Path, executable: &Path, app_id: &str) -> windows::core::Result<()> {
    let link: IShellLinkW = unsafe { CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)? };
    let file: IPersistFile = link.cast()?;
    let properties: IPropertyStore = link.cast()?;
    let destination = HSTRING::from(shortcut);
    if shortcut.try_exists()? {
        // Preserve the installer's shortcut and any user customization. Never replace
        // an unrelated or unreadable entry just because its filename is Jarvis.lnk.
        unsafe { file.Load(&destination, STGM_READ)? };
        let existing = unsafe { properties.GetValue(&PKEY_AppUserModel_ID)? };
        if existing.to_string() != app_id {
            return Err(windows::core::Error::from_hresult(windows::core::HRESULT(
                0x800700B7_u32 as i32,
            )));
        }
        return Ok(());
    }
    if !executable.is_file() {
        return Err(
            std::io::Error::new(std::io::ErrorKind::NotFound, "Missing Jarvis executable").into(),
        );
    }
    let parent = shortcut
        .parent()
        .ok_or_else(|| std::io::Error::other("Missing shortcut directory"))?;
    fs::create_dir_all(parent)?;
    let target =
        HSTRING::from(crate::library::strip_verbatim(&executable.to_string_lossy()).as_ref());
    unsafe {
        link.SetPath(&target)?;
        link.SetDescription(&HSTRING::from("Jarvis"))?;
        link.SetIconLocation(&target, 0)?;
        properties.SetValue(&PKEY_AppUserModel_ID, &PROPVARIANT::from(app_id))?;
        properties.Commit()?;
        file.Save(&destination, true)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcut_exposes_jarvis_identity_and_preserves_existing_entry() {
        let _apartment = super::super::Apartment::new().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("Jarvis ação.exe");
        fs::write(&target, []).unwrap();
        let path = directory.path().join("Programs/Jarvis/Jarvis.lnk");
        ensure(&path, &target, "jarvis.test").unwrap();
        let before = fs::read(&path).unwrap();
        ensure(&path, &target, "jarvis.test").unwrap();
        assert_eq!(fs::read(&path).unwrap(), before);
        assert!(ensure(&path, &target, "another.app").is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
        let link: IShellLinkW =
            unsafe { CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).unwrap() };
        let file: IPersistFile = link.cast().unwrap();
        unsafe {
            file.Load(&HSTRING::from(path.as_path()), STGM_READ)
                .unwrap()
        };
        let mut actual = [0_u16; 32768];
        unsafe { link.GetPath(&mut actual, std::ptr::null_mut(), 0).unwrap() };
        let length = actual.iter().position(|v| *v == 0).unwrap();
        // ShellLink expands 8.3 aliases (for example RUNNER~1 in CI's TEMP).
        // Compare the referenced file, not two spellings of the same path.
        assert_eq!(
            fs::canonicalize(Path::new(&String::from_utf16(&actual[..length]).unwrap())).unwrap(),
            fs::canonicalize(target).unwrap()
        );
    }
}
