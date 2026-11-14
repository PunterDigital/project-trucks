//! Where the Hub token lives: Windows Credential Manager for a real install, a file in the data
//! folder for test profiles (`PROJECTTRUCKS_DATA_DIR`), so tests never touch the user's
//! credentials.

use std::path::PathBuf;

/// Credential Manager entry name ("Windows Credentials" → "Generic Credentials").
#[cfg(windows)]
const CREDENTIAL_TARGET: &str = "ProjectTrucks/hub-token";

#[derive(Debug, Clone)]
pub enum TokenStore {
    #[cfg(windows)]
    Credential,
    File(PathBuf),
}

impl TokenStore {
    /// `test_dir` is the test profile's data folder, if this is one; `fallback_dir` is used on
    /// platforms without Credential Manager.
    pub fn new(test_dir: Option<PathBuf>, fallback_dir: PathBuf) -> Self {
        match test_dir {
            Some(dir) => TokenStore::File(dir.join("hub-token")),
            #[cfg(windows)]
            None => {
                let _ = fallback_dir;
                TokenStore::Credential
            }
            #[cfg(not(windows))]
            None => TokenStore::File(fallback_dir.join("hub-token")),
        }
    }

    pub fn get(&self) -> Option<String> {
        let token = match self {
            #[cfg(windows)]
            TokenStore::Credential => credential::read(CREDENTIAL_TARGET),
            TokenStore::File(path) => std::fs::read_to_string(path).ok(),
        }?;
        let token = token.trim().to_string();
        (!token.is_empty()).then_some(token)
    }

    pub fn set(&self, token: &str) -> Result<(), String> {
        match self {
            #[cfg(windows)]
            TokenStore::Credential => credential::write(CREDENTIAL_TARGET, token),
            TokenStore::File(path) => std::fs::write(path, token).map_err(|e| e.to_string()),
        }
    }

    pub fn delete(&self) {
        match self {
            #[cfg(windows)]
            TokenStore::Credential => credential::delete(CREDENTIAL_TARGET),
            TokenStore::File(path) => {
                let _ = std::fs::remove_file(path);
            }
        }
    }
}

#[cfg(windows)]
mod credential {
    use std::ptr::null_mut;

    use windows_sys::Win32::Foundation::FILETIME;
    use windows_sys::Win32::Security::Credentials::{
        CredDeleteW, CredFree, CredReadW, CredWriteW, CREDENTIALW, CRED_PERSIST_LOCAL_MACHINE,
        CRED_TYPE_GENERIC,
    };

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    pub fn write(target: &str, secret: &str) -> Result<(), String> {
        let mut target_w = wide(target);
        let mut user_w = wide("ProjectTrucks");
        let mut blob = secret.as_bytes().to_vec();
        let credential = CREDENTIALW {
            Flags: 0,
            Type: CRED_TYPE_GENERIC,
            TargetName: target_w.as_mut_ptr(),
            Comment: null_mut(),
            LastWritten: FILETIME {
                dwLowDateTime: 0,
                dwHighDateTime: 0,
            },
            CredentialBlobSize: blob.len() as u32,
            CredentialBlob: blob.as_mut_ptr(),
            Persist: CRED_PERSIST_LOCAL_MACHINE,
            AttributeCount: 0,
            Attributes: null_mut(),
            TargetAlias: null_mut(),
            UserName: user_w.as_mut_ptr(),
        };
        // SAFETY: every pointer in `credential` points into a buffer that outlives the call.
        let ok = unsafe { CredWriteW(&credential, 0) };
        if ok == 0 {
            Err(format!(
                "couldn't save the Hub token in Windows Credential Manager ({})",
                std::io::Error::last_os_error()
            ))
        } else {
            Ok(())
        }
    }

    pub fn read(target: &str) -> Option<String> {
        let target_w = wide(target);
        let mut found: *mut CREDENTIALW = null_mut();
        // SAFETY: `target_w` is NUL-terminated; on success Windows allocates `found`, freed below.
        let ok = unsafe { CredReadW(target_w.as_ptr(), CRED_TYPE_GENERIC, 0, &mut found) };
        if ok == 0 || found.is_null() {
            return None;
        }
        // SAFETY: `found` is a valid CREDENTIALW from CredReadW; the blob is `CredentialBlobSize`
        // bytes long. Copied out before CredFree.
        let secret = unsafe {
            let c = &*found;
            let bytes = if c.CredentialBlob.is_null() {
                Vec::new()
            } else {
                std::slice::from_raw_parts(c.CredentialBlob, c.CredentialBlobSize as usize).to_vec()
            };
            CredFree(found as *const _);
            bytes
        };
        String::from_utf8(secret).ok()
    }

    pub fn delete(target: &str) {
        let target_w = wide(target);
        // SAFETY: `target_w` is NUL-terminated. A missing entry just fails, which is fine.
        unsafe {
            CredDeleteW(target_w.as_ptr(), CRED_TYPE_GENERIC, 0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_store_round_trip() {
        let dir = std::env::temp_dir().join(format!("pt-hub-token-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = TokenStore::new(Some(dir.clone()), dir.clone());
        assert_eq!(store.get(), None);
        store.set("1|abc").unwrap();
        assert_eq!(store.get().as_deref(), Some("1|abc"));
        store.delete();
        assert_eq!(store.get(), None);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
