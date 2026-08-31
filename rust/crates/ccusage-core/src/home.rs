use std::{cell::RefCell, env, ffi::OsString, path::PathBuf};

thread_local! {
    static HOME_DIR_OVERRIDE: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}

struct HomeDirOverrideGuard(Option<PathBuf>);

impl Drop for HomeDirOverrideGuard {
    fn drop(&mut self) {
        HOME_DIR_OVERRIDE.with(|slot| {
            slot.replace(self.0.take());
        });
    }
}

pub fn home_dir() -> Option<PathBuf> {
    if let Some(home) = HOME_DIR_OVERRIDE.with(|slot| slot.borrow().clone()) {
        return Some(home);
    }
    home_dir_from_env(
        env::var_os("HOME"),
        env::var_os("USERPROFILE"),
        env::var_os("HOMEDRIVE"),
        env::var_os("HOMEPATH"),
    )
}

pub fn with_home_dir_override<T>(home: PathBuf, operation: impl FnOnce() -> T) -> T {
    let previous = HOME_DIR_OVERRIDE.with(|slot| slot.replace(Some(home)));
    let _guard = HomeDirOverrideGuard(previous);
    operation()
}

pub fn home_dir_is_overridden() -> bool {
    HOME_DIR_OVERRIDE.with(|slot| slot.borrow().is_some())
}

pub fn data_path_env_var(name: &str) -> Result<String, env::VarError> {
    if home_dir_is_overridden() {
        return Err(env::VarError::NotPresent);
    }
    env::var(name)
}

pub fn data_path_env_var_os(name: &str) -> Option<OsString> {
    if home_dir_is_overridden() {
        return None;
    }
    env::var_os(name)
}

fn home_dir_from_env(
    home: Option<std::ffi::OsString>,
    user_profile: Option<std::ffi::OsString>,
    home_drive: Option<std::ffi::OsString>,
    home_path: Option<std::ffi::OsString>,
) -> Option<PathBuf> {
    if let Some(path) = non_empty_path(home) {
        return Some(path);
    }
    if let Some(path) = non_empty_path(user_profile) {
        return Some(path);
    }
    let mut drive = home_drive?;
    let path = home_path?;
    if drive.is_empty() || path.is_empty() {
        return None;
    }
    drive.push(path);
    Some(PathBuf::from(drive))
}

fn non_empty_path(path: Option<std::ffi::OsString>) -> Option<PathBuf> {
    let path = path?;
    (!path.is_empty()).then(|| PathBuf::from(path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ccusage_test_support::EnvVarGuard;
    use std::ffi::OsString;

    #[test]
    fn prefers_home_when_available() {
        let path = home_dir_from_env(
            Some(OsString::from("/home/user")),
            Some(OsString::from("C:\\Users\\runner")),
            None,
            None,
        );
        assert_eq!(path, Some(PathBuf::from("/home/user")));
    }

    #[test]
    fn falls_back_to_windows_user_profile_without_home() {
        let path = home_dir_from_env(None, Some(OsString::from("C:\\Users\\runner")), None, None);
        assert_eq!(path, Some(PathBuf::from("C:\\Users\\runner")));
    }

    #[test]
    fn falls_back_to_windows_home_drive_and_path() {
        let path = home_dir_from_env(
            None,
            None,
            Some(OsString::from("C:")),
            Some(OsString::from("\\Users\\runner")),
        );
        assert_eq!(path, Some(PathBuf::from("C:\\Users\\runner")));
    }

    #[test]
    fn scoped_override_replaces_home_and_restores_it_after_return() {
        let before = home_dir();

        let inside = with_home_dir_override(PathBuf::from("/srv/users/alice"), || {
            assert!(home_dir_is_overridden());
            home_dir()
        });

        assert_eq!(inside, Some(PathBuf::from("/srv/users/alice")));
        assert!(!home_dir_is_overridden());
        assert_eq!(home_dir(), before);
    }

    #[test]
    fn scoped_override_restores_home_after_panic() {
        let before = home_dir();

        let _ = std::panic::catch_unwind(|| {
            with_home_dir_override(PathBuf::from("/srv/users/alice"), || panic!("boom"));
        });

        assert!(!home_dir_is_overridden());
        assert_eq!(home_dir(), before);
    }

    #[test]
    fn scoped_override_hides_data_path_environment_variables() {
        let _env = EnvVarGuard::set("CCUSAGE_TEST_DATA_HOME", "/custom/data");
        assert_eq!(
            data_path_env_var("CCUSAGE_TEST_DATA_HOME").as_deref(),
            Ok("/custom/data")
        );

        with_home_dir_override(PathBuf::from("/srv/users/alice"), || {
            assert!(data_path_env_var("CCUSAGE_TEST_DATA_HOME").is_err());
            assert!(data_path_env_var_os("CCUSAGE_TEST_DATA_HOME").is_none());
        });
    }
}
