use std::path::PathBuf;

use crate::{Result, cli_error};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct SystemUser {
    pub(super) name: String,
    pub(super) home: PathBuf,
}

#[cfg(any(target_os = "linux", test))]
impl SystemUser {
    pub(super) fn new(name: impl Into<String>, home: PathBuf) -> Self {
        Self {
            name: name.into(),
            home,
        }
    }
}

#[cfg(target_os = "linux")]
#[derive(Debug)]
struct UserRecord {
    name: String,
    uid: u32,
    home: PathBuf,
}

#[cfg(target_os = "linux")]
impl UserRecord {
    fn new(name: impl Into<String>, uid: u32, home: PathBuf) -> Self {
        Self {
            name: name.into(),
            uid,
            home,
        }
    }
}

#[cfg(target_os = "linux")]
pub(super) fn discover_system_users() -> Result<Vec<SystemUser>> {
    require_root(unsafe { libc::geteuid() })?;
    let users = select_system_users(read_user_records()?);
    if users.is_empty() {
        return Err(cli_error(
            "--all-users found no system users with an existing home directory",
        ));
    }
    Ok(users)
}

#[cfg(target_os = "linux")]
fn require_root(effective_uid: u32) -> Result<()> {
    if effective_uid == 0 {
        return Ok(());
    }
    Err(cli_error("--all-users requires root privileges on Linux"))
}

#[cfg(not(target_os = "linux"))]
pub(super) fn discover_system_users() -> Result<Vec<SystemUser>> {
    Err(cli_error("--all-users is only supported on Linux"))
}

#[cfg(target_os = "linux")]
fn read_user_records() -> Result<Vec<UserRecord>> {
    use std::{ffi::CStr, os::unix::ffi::OsStringExt};

    struct PasswdGuard;
    impl Drop for PasswdGuard {
        fn drop(&mut self) {
            unsafe { libc::endpwent() };
        }
    }

    unsafe { libc::setpwent() };
    let _guard = PasswdGuard;
    let mut records = Vec::new();
    loop {
        // `getpwent` is exposed and linkable on both glibc and musl. Account
        // discovery runs before agent loader threads start, and every field is
        // copied before the next call can reuse libc's static buffer.
        unsafe { *libc::__errno_location() = 0 };
        let passwd = unsafe { libc::getpwent() };
        if passwd.is_null() {
            let errno = unsafe { *libc::__errno_location() };
            if passwd_errno_is_end(errno) {
                break;
            }
            return Err(cli_error(format!(
                "failed to enumerate Linux system users: {}",
                std::io::Error::from_raw_os_error(errno)
            )));
        }
        let passwd = unsafe { &*passwd };
        if passwd.pw_name.is_null() || passwd.pw_dir.is_null() {
            continue;
        }
        let name = unsafe { CStr::from_ptr(passwd.pw_name) }
            .to_string_lossy()
            .into_owned();
        let home = PathBuf::from(std::ffi::OsString::from_vec(
            unsafe { CStr::from_ptr(passwd.pw_dir) }.to_bytes().to_vec(),
        ));
        records.push(UserRecord::new(name, passwd.pw_uid, home));
    }
    Ok(records)
}

#[cfg(target_os = "linux")]
fn passwd_errno_is_end(errno: i32) -> bool {
    errno == 0 || errno == libc::ENOENT
}

#[cfg(target_os = "linux")]
fn select_system_users(records: Vec<UserRecord>) -> Vec<SystemUser> {
    use std::{collections::BTreeMap, os::unix::fs::MetadataExt};

    let mut homes = BTreeMap::<PathBuf, Vec<UserRecord>>::new();
    for mut record in records {
        if !record.home.is_dir() {
            continue;
        }
        let Ok(home) = record.home.canonicalize() else {
            continue;
        };
        record.home = home.clone();
        homes.entry(home).or_default().push(record);
    }

    let mut users = homes
        .into_iter()
        .map(|(home, mut records)| {
            records.sort_by(|left, right| left.name.cmp(&right.name));
            let owner = std::fs::metadata(&home).ok().map(|metadata| metadata.uid());
            let selected = owner
                .and_then(|uid| records.iter().position(|record| record.uid == uid))
                .unwrap_or(0);
            let record = records.swap_remove(selected);
            SystemUser::new(record.name, home)
        })
        .collect::<Vec<_>>();
    users.sort_by(|left, right| left.name.cmp(&right.name).then(left.home.cmp(&right.home)));
    users
}

#[cfg(test)]
mod tests {
    use super::*;
    use ccusage_test_support::Fixture;

    #[cfg(target_os = "linux")]
    use std::os::unix::fs::MetadataExt;

    #[cfg(target_os = "linux")]
    #[test]
    fn selects_existing_unique_homes_and_prefers_the_filesystem_owner() {
        let fixture = Fixture::new();
        let shared_home = fixture.create_dir_all("shared");
        let other_home = fixture.create_dir_all("other");
        let owner_uid = std::fs::metadata(&shared_home).unwrap().uid();
        let records = vec![
            UserRecord::new("z-alias", owner_uid + 1, shared_home.clone()),
            UserRecord::new("owner", owner_uid, shared_home.clone()),
            UserRecord::new("other", owner_uid + 2, other_home.clone()),
            UserRecord::new("missing", owner_uid + 3, fixture.path("missing")),
        ];

        let users = select_system_users(records);

        assert_eq!(
            users,
            vec![
                SystemUser::new("other", other_home.canonicalize().unwrap()),
                SystemUser::new("owner", shared_home.canonicalize().unwrap()),
            ]
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn rejects_non_root_effective_user() {
        let error = require_root(1_000).unwrap_err();

        assert_eq!(
            error.to_string(),
            "--all-users requires root privileges on Linux"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn treats_linux_passwd_zero_and_enoent_as_end_of_enumeration() {
        assert!(passwd_errno_is_end(0));
        assert!(passwd_errno_is_end(libc::ENOENT));
        assert!(!passwd_errno_is_end(libc::EIO));
    }
}
