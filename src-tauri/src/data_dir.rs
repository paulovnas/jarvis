//! Runtime data-directory selection and process-wide ownership.

use fs2::FileExt;
use std::{
    ffi::OsStr,
    fs::{self, File, Metadata, OpenOptions},
    io::{self, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::OnceLock,
};

pub(crate) const PRODUCTION_IDENTIFIER: &str = "com.foxtag.jarvis";
pub(crate) const DEVELOPMENT_IDENTIFIER: &str = "com.foxtag.jarvis.dev";
const PRODUCTION_DIRECTORY: &str = ".jarvis";
const DEVELOPMENT_DIRECTORY: &str = ".jarvis-dev";
const LEASE_FILE: &str = ".instance.lock";
const PROFILE_ENV: &str = "JARVIS_RUNTIME_PROFILE";
static ACTIVE_PROFILE: OnceLock<Profile> = OnceLock::new();

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Profile {
    Production,
    Development,
}

impl Profile {
    const fn directory(self) -> &'static str {
        match self {
            Self::Production => PRODUCTION_DIRECTORY,
            Self::Development => DEVELOPMENT_DIRECTORY,
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Production => "production",
            Self::Development => "development",
        }
    }
}

fn selected_profile(identifier: Option<&str>, debug: bool, requested: Option<&OsStr>) -> Profile {
    if identifier == Some(DEVELOPMENT_IDENTIFIER)
        || debug
        || requested.is_some_and(|value| value == "development")
    {
        Profile::Development
    } else {
        debug_assert!(identifier.is_none() || identifier == Some(PRODUCTION_IDENTIFIER));
        Profile::Production
    }
}

pub(crate) fn profile() -> Profile {
    ACTIVE_PROFILE.get().copied().unwrap_or_else(|| {
        selected_profile(
            None,
            cfg!(debug_assertions),
            std::env::var_os(PROFILE_ENV).as_deref(),
        )
    })
}

pub(crate) fn configure(identifier: &str) -> io::Result<Profile> {
    let selected = selected_profile(
        Some(identifier),
        cfg!(debug_assertions),
        std::env::var_os(PROFILE_ENV).as_deref(),
    );
    if let Some(active) = ACTIVE_PROFILE.get() {
        if *active != selected {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "O perfil de dados do Jarvis já foi configurado.",
            ));
        }
        return Ok(*active);
    }
    let _ = ACTIVE_PROFILE.set(selected);
    Ok(selected)
}

pub(crate) fn root_for(home: &Path, profile: Profile) -> PathBuf {
    home.join(profile.directory())
}

pub(crate) fn root(home: &Path) -> PathBuf {
    root_for(home, profile())
}

#[cfg(target_os = "macos")]
pub(crate) fn keychain_service(
    production: &'static str,
    development: &'static str,
) -> &'static str {
    match profile() {
        Profile::Production => production,
        Profile::Development => development,
    }
}

fn redirected(metadata: &Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes()
            & windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT
            != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

fn invalid_path(path: &Path) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!(
            "O caminho de dados do Jarvis não é uma pasta ou arquivo comum: {}",
            path.display()
        ),
    )
}

fn ensure_directory(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() && !redirected(&metadata) => return Ok(()),
        Ok(_) => return Err(invalid_path(path)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }

    let builder = {
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            let mut builder = fs::DirBuilder::new();
            builder.mode(0o700);
            builder
        }
        #[cfg(not(unix))]
        {
            fs::DirBuilder::new()
        }
    };
    match builder.create(path) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error),
    }
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_dir() || redirected(&metadata) {
        return Err(invalid_path(path));
    }
    Ok(())
}

fn open_lease(path: &Path) -> io::Result<File> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() && !redirected(&metadata) => {}
        Ok(_) => return Err(invalid_path(path)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }

    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let file = options.open(path)?;
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() || redirected(&metadata) {
        return Err(invalid_path(path));
    }
    Ok(file)
}

#[derive(Debug)]
pub(crate) struct Lease {
    _file: File,
    root: PathBuf,
}

impl Lease {
    pub(crate) fn acquire(home: &Path, profile: Profile) -> io::Result<Self> {
        let root = root_for(home, profile);
        ensure_directory(&root)?;
        let path = root.join(LEASE_FILE);
        let mut file = open_lease(&path)?;
        FileExt::try_lock_exclusive(&file).map_err(|error| {
            if error.kind() == fs2::lock_contended_error().kind() {
                io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    format!(
                        "Outra instância do Jarvis já está usando {}.",
                        root.display()
                    ),
                )
            } else {
                error
            }
        })?;
        file.set_len(0)?;
        file.seek(SeekFrom::Start(0))?;
        writeln!(file, "pid={}", std::process::id())?;
        writeln!(file, "profile={}", profile.label())?;
        file.sync_data()?;
        let metadata = fs::symlink_metadata(&root)?;
        if !metadata.is_dir() || redirected(&metadata) {
            return Err(invalid_path(&root));
        }
        Ok(Self { _file: file, root })
    }

    pub(crate) fn root(&self) -> &Path {
        &self.root
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn development_and_production_use_distinct_roots() {
        let home = Path::new("/home/example");
        assert_eq!(
            root_for(home, Profile::Production),
            home.join(PRODUCTION_DIRECTORY)
        );
        assert_eq!(
            root_for(home, Profile::Development),
            home.join(DEVELOPMENT_DIRECTORY)
        );
        assert_ne!(
            root_for(home, Profile::Production),
            root_for(home, Profile::Development)
        );
        assert_eq!(
            selected_profile(None, false, Some(OsStr::new("development"))),
            Profile::Development
        );
        assert_eq!(
            selected_profile(Some(PRODUCTION_IDENTIFIER), false, None),
            Profile::Production
        );
        assert_eq!(
            selected_profile(Some(DEVELOPMENT_IDENTIFIER), false, None),
            Profile::Development
        );
        assert_eq!(selected_profile(None, true, None), Profile::Development);
    }

    #[test]
    fn lease_is_exclusive_and_released_with_its_owner() {
        let home = tempfile::tempdir().unwrap();
        let first = Lease::acquire(home.path(), Profile::Production).unwrap();
        assert_eq!(first.root(), home.path().join(PRODUCTION_DIRECTORY));
        assert_eq!(
            Lease::acquire(home.path(), Profile::Production)
                .unwrap_err()
                .kind(),
            io::ErrorKind::AlreadyExists
        );
        drop(first);
        Lease::acquire(home.path(), Profile::Production).unwrap();
    }

    #[test]
    fn distinct_profiles_can_hold_their_leases_together() {
        let home = tempfile::tempdir().unwrap();
        let production = Lease::acquire(home.path(), Profile::Production).unwrap();
        let development = Lease::acquire(home.path(), Profile::Development).unwrap();
        assert_ne!(production.root(), development.root());
    }

    #[cfg(unix)]
    #[test]
    fn redirected_data_root_and_lease_are_rejected() {
        use std::os::unix::fs::symlink;

        let home = tempfile::tempdir().unwrap();
        let external = tempfile::tempdir().unwrap();
        let root = root_for(home.path(), Profile::Production);
        symlink(external.path(), &root).unwrap();
        assert_eq!(
            Lease::acquire(home.path(), Profile::Production)
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidData
        );

        fs::remove_file(&root).unwrap();
        fs::create_dir(&root).unwrap();
        let target = external.path().join("lease");
        fs::write(&target, []).unwrap();
        symlink(target, root.join(LEASE_FILE)).unwrap();
        assert_eq!(
            Lease::acquire(home.path(), Profile::Production)
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidData
        );
    }

    #[cfg(windows)]
    #[test]
    fn redirected_windows_data_root_is_rejected() {
        let home = tempfile::tempdir().unwrap();
        let external = tempfile::tempdir().unwrap();
        let root = root_for(home.path(), Profile::Production);
        junction::create(external.path(), &root).unwrap();
        assert_eq!(
            Lease::acquire(home.path(), Profile::Production)
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidData
        );
        junction::delete(root).unwrap();
    }
}
