//! Cloud backup secrets live in the OS credential store, never in settings.
use crate::plugin::builtin::cloud_backup::CloudBackupConfig;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

const SERVICE: &str = "paper.shell.app.cloud-backup.v1";

trait Store {
    fn get(&self, id: &str) -> Result<Zeroizing<String>, String>;
    fn set(&self, id: &str, value: &str) -> Result<(), String>;
    fn delete(&self, id: &str) -> Result<(), String>;
}

struct SystemStore;

fn entry(id: &str) -> Result<keyring::Entry, String> {
    // Never silently use keyring's mock backend on an unsupported platform.
    if !cfg!(any(
        target_os = "macos",
        target_os = "windows",
        target_os = "linux"
    )) {
        return Err("此平台尚不支持系统凭据库".into());
    }
    keyring::Entry::new(SERVICE, id).map_err(|_| "无法打开系统凭据库".into())
}

impl Store for SystemStore {
    fn get(&self, id: &str) -> Result<Zeroizing<String>, String> {
        entry(id)?
            .get_password()
            .map(Zeroizing::new)
            .map_err(|_| "无法读取已保存的密码，请解锁系统凭据库或重新输入并保存".into())
    }
    fn set(&self, id: &str, value: &str) -> Result<(), String> {
        entry(id)?
            .set_password(value)
            .map_err(|_| "无法保存密码到系统凭据库，请解锁后重试；配置尚未保存".into())
    }
    fn delete(&self, id: &str) -> Result<(), String> {
        match entry(id)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => {
                Err("无法清理旧的系统凭据，请在系统凭据管理器中清理 Paper Shell 云备份条目".into())
            }
        }
    }
}

#[derive(Serialize, Deserialize)]
struct Secrets {
    server_url: String,
    username: String,
    password: String,
    encryption_password: String,
}

impl Drop for Secrets {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.password.zeroize();
        self.encryption_password.zeroize();
    }
}

pub(crate) fn restore(config: &mut CloudBackupConfig) {
    if let Err(error) = restore_with(config, &SystemStore) {
        config.credential_error = Some(error);
    }
}

fn restore_with(config: &mut CloudBackupConfig, store: &impl Store) -> Result<(), String> {
    let Some(id) = &config.credential_id else {
        return Ok(());
    };
    let encoded = store.get(id)?;
    let secrets: Secrets = serde_json::from_str(&encoded)
        .map_err(|_| "已保存的凭据格式无效，请重新输入并保存".to_string())?;
    // Never send a saved credential to a manually edited endpoint/account.
    if secrets.server_url != config.server_url.trim() || secrets.username != config.username {
        return Err("云端地址或用户名已改变，请重新输入密码并保存".into());
    }
    config.password = Zeroizing::new(secrets.password.clone());
    config.encryption_password = Zeroizing::new(secrets.encryption_password.clone());
    config.credential_error = None;
    Ok(())
}

/// Save a new entry before committing its reference. On failure the previous
/// settings and entry remain usable. Empty fields remove the saved secrets.
pub(crate) fn save(
    config: CloudBackupConfig,
    persist: impl FnOnce(&CloudBackupConfig) -> Result<(), String>,
) -> Result<(CloudBackupConfig, Option<String>), String> {
    save_with(config, &SystemStore, persist)
}

fn save_with(
    mut config: CloudBackupConfig,
    store: &impl Store,
    persist: impl FnOnce(&CloudBackupConfig) -> Result<(), String>,
) -> Result<(CloudBackupConfig, Option<String>), String> {
    let previous_id = config.credential_id.take();
    if !config.password.is_empty() || !config.encryption_password.is_empty() {
        let secrets = Secrets {
            server_url: config.server_url.trim().into(),
            username: config.username.clone(),
            password: config.password.to_string(),
            encryption_password: config.encryption_password.to_string(),
        };
        let encoded = Zeroizing::new(
            serde_json::to_string(&secrets).map_err(|_| "无法编码云端凭据".to_string())?,
        );
        let id = uuid::Uuid::new_v4().to_string();
        store.set(&id, &encoded)?;
        config.credential_id = Some(id);
    }
    config.credential_error = None;
    if let Err(error) = persist(&config) {
        if let Some(id) = &config.credential_id
            && let Err(cleanup_error) = store.delete(id)
        {
            return Err(format!("{error}；{cleanup_error}"));
        }
        return Err(error);
    }
    let warning = previous_id.and_then(|id| store.delete(&id).err());
    Ok((config, warning))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::RefCell, collections::HashMap};

    #[derive(Default)]
    struct MemoryStore {
        entries: RefCell<HashMap<String, String>>,
        locked: bool,
    }
    impl Store for MemoryStore {
        fn get(&self, id: &str) -> Result<Zeroizing<String>, String> {
            self.entries
                .borrow()
                .get(id)
                .cloned()
                .map(Zeroizing::new)
                .ok_or("missing".into())
        }
        fn set(&self, id: &str, value: &str) -> Result<(), String> {
            if self.locked {
                return Err("locked".into());
            }
            self.entries.borrow_mut().insert(id.into(), value.into());
            Ok(())
        }
        fn delete(&self, id: &str) -> Result<(), String> {
            self.entries.borrow_mut().remove(id);
            Ok(())
        }
    }
    fn config() -> CloudBackupConfig {
        CloudBackupConfig {
            server_url: "https://example.com/dav/".into(),
            password: Zeroizing::new("login-secret".into()),
            encryption_password: Zeroizing::new("master-secret".into()),
            ..Default::default()
        }
    }
    #[test]
    fn restart_restores_both_secrets_without_plaintext_settings() {
        let store = MemoryStore::default();
        let mut disk = String::new();
        save_with(config(), &store, |c| {
            disk = toml::to_string(c).unwrap();
            Ok(())
        })
        .unwrap();
        assert!(!disk.contains("login-secret"));
        assert!(!disk.contains("master-secret"));
        let mut loaded = toml::from_str(&disk).unwrap();
        restore_with(&mut loaded, &store).unwrap();
        assert_eq!(*loaded.password, "login-secret");
        assert_eq!(*loaded.encryption_password, "master-secret");
        loaded.server_url = "https://other.example/".into();
        loaded.password.clear();
        loaded.encryption_password.clear();
        assert!(restore_with(&mut loaded, &store).is_err());
        assert!(loaded.password.is_empty());
    }
    #[test]
    fn failed_save_keeps_previous_entry_and_clear_removes_it() {
        let store = MemoryStore::default();
        let (mut saved, _) = save_with(config(), &store, |_| Ok(())).unwrap();
        let id = saved.credential_id.clone().unwrap();
        saved.password = Zeroizing::new("changed".into());
        assert!(save_with(saved.clone(), &store, |_| Err("disk full".into())).is_err());
        assert_eq!(store.entries.borrow().len(), 1);
        restore_with(&mut saved, &store).unwrap();
        assert_eq!(*saved.password, "login-secret");
        saved.password.clear();
        saved.encryption_password.clear();
        let (cleared, _) = save_with(saved, &store, |_| Ok(())).unwrap();
        assert!(cleared.credential_id.is_none());
        assert!(store.entries.borrow().get(&id).is_none());
    }
    #[test]
    fn locked_store_does_not_commit_settings_and_missing_entry_is_reported() {
        let store = MemoryStore {
            locked: true,
            ..Default::default()
        };
        assert!(save_with(config(), &store, |_| panic!("must not persist")).is_err());
        let mut saved = CloudBackupConfig {
            credential_id: Some("missing".into()),
            ..Default::default()
        };
        assert!(restore_with(&mut saved, &store).is_err());
    }

    #[test]
    fn saving_changed_secrets_replaces_old_entry_and_preserves_master_when_disabled() {
        let store = MemoryStore::default();
        let (mut saved, _) = save_with(config(), &store, |_| Ok(())).unwrap();
        let old_id = saved.credential_id.clone().unwrap();
        saved.password = Zeroizing::new("new-login".into());
        saved.enable_encryption = false;
        let (saved, warning) = save_with(saved, &store, |_| Ok(())).unwrap();
        assert!(warning.is_none());
        assert!(!store.entries.borrow().contains_key(&old_id));
        assert_eq!(store.entries.borrow().len(), 1);
        let mut loaded = toml::from_str(&toml::to_string(&saved).unwrap()).unwrap();
        restore_with(&mut loaded, &store).unwrap();
        assert_eq!(*loaded.password, "new-login");
        assert_eq!(*loaded.encryption_password, "master-secret");
    }

    #[test]
    #[ignore = "requires an unlocked OS credential store; writes and deletes a temporary test entry"]
    fn system_store_roundtrip() {
        let (saved, _) = save(config(), |_| Ok(())).unwrap();
        let mut loaded = toml::from_str(&toml::to_string(&saved).unwrap()).unwrap();
        let result = restore_with(&mut loaded, &SystemStore);
        let cleanup = SystemStore.delete(saved.credential_id.as_deref().unwrap());
        result.unwrap();
        cleanup.unwrap();
        assert_eq!(*loaded.password, "login-secret");
        assert_eq!(*loaded.encryption_password, "master-secret");
    }
}
