use super::db::Database;
use crate::domain::advanced::{CredentialGrant, SaveCredential, authorized, validate_scope};
use crate::domain::{AppError, ErrorCode};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Serialize, Deserialize)]
pub struct Secret {
    pub username: String,
    pub password: String,
}
fn vault_error() -> AppError {
    AppError::new(
        ErrorCode::StoragePermission,
        "The OS credential store is unavailable or access was denied. Unlock it and try again.",
    )
}
fn entry(db: &Database, id: &str) -> Result<keyring::Entry, AppError> {
    let scope = hex::encode(Sha256::digest(db.path().to_string_lossy().as_bytes()));
    keyring::Entry::new(&format!("circle-downloader.{scope}"), id).map_err(|_| vault_error())
}
pub fn save(db: &Database, input: SaveCredential) -> Result<CredentialGrant, AppError> {
    let scope = validate_scope(&input.scope, &input.kind)?;
    if input.username.len() > 256
        || input.password.is_empty()
        || input.password.len() > 4096
        || input.username.chars().any(char::is_control)
    {
        return Err(AppError::new(
            ErrorCode::InvalidTransition,
            "Use a username of up to 256 characters and a nonempty password of up to 4096 characters.",
        ));
    }
    let grant = CredentialGrant {
        id: uuid::Uuid::new_v4().to_string(),
        kind: input.kind,
        scope,
    };
    let secret = serde_json::to_string(&Secret {
        username: input.username,
        password: input.password,
    })
    .map_err(|_| vault_error())?;
    let vault = entry(db, &grant.id)?;
    vault.set_password(&secret).map_err(|_| vault_error())?;
    if let Err(e) = db.save_grant(&grant) {
        let _ = vault.delete_credential();
        return Err(e);
    }
    Ok(grant)
}
pub fn load(db: &Database, id: &str, kind: &str, url: &str) -> Result<Secret, AppError> {
    let grant = db
        .grants()?
        .into_iter()
        .find(|g| g.id == id)
        .ok_or_else(|| {
            AppError::new(
                ErrorCode::InvalidTransition,
                "The selected credential authorization has been revoked.",
            )
        })?;
    if !authorized(&grant, kind, url) {
        return Err(AppError::new(
            ErrorCode::UrlNotAllowed,
            "Credential authorization does not match this exact origin.",
        ));
    }
    let raw = entry(db, id)?.get_password().map_err(|_| vault_error())?;
    serde_json::from_str(&raw).map_err(|_| vault_error())
}
pub fn revoke(db: &Database, id: &str) -> Result<(), AppError> {
    if !db.grants()?.iter().any(|g| g.id == id) {
        return Err(AppError::new(
            ErrorCode::InvalidTransition,
            "Credential authorization not found.",
        ));
    }
    match entry(db, id)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => {}
        Err(_) => return Err(vault_error()),
    }
    db.revoke_grant(id)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires an unlocked OS credential store; creates and removes an isolated test entry"]
    fn os_store_roundtrip_revocation_and_database_redaction() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("vault-test.db");
        let db = Database::open(&path).unwrap();
        let grant = save(
            &db,
            SaveCredential {
                kind: "origin".into(),
                scope: "https://example.com".into(),
                username: "qa-user".into(),
                password: "qa-secret-must-not-be-in-db".into(),
            },
        )
        .unwrap();
        let secret = load(&db, &grant.id, "origin", "https://example.com/file").unwrap();
        // Remove first so later assertions cannot leave test credentials behind.
        revoke(&db, &grant.id).unwrap();
        assert_eq!(secret.username, "qa-user");
        assert_eq!(secret.password, "qa-secret-must-not-be-in-db");
        assert!(load(&db, &grant.id, "origin", "https://example.com/file").is_err());
        assert!(db.grants().unwrap().is_empty());
        for item in std::fs::read_dir(dir.path()).unwrap() {
            let bytes = std::fs::read(item.unwrap().path()).unwrap();
            assert!(
                !bytes
                    .windows(b"qa-secret-must-not-be-in-db".len())
                    .any(|v| v == b"qa-secret-must-not-be-in-db")
            );
        }
    }

    #[test]
    fn unauthorized_access_is_rejected_before_os_store_and_errors_are_redacted() {
        let db = Database::open_in_memory().unwrap();
        let g = CredentialGrant {
            id: "test".into(),
            kind: "origin".into(),
            scope: "https://example.com".into(),
        };
        db.save_grant(&g).unwrap();
        let e = match load(&db, "test", "origin", "https://other.com/?password=private") {
            Err(e) => e,
            Ok(_) => panic!("unauthorized"),
        };
        assert_eq!(e.code, ErrorCode::UrlNotAllowed);
        let text = serde_json::to_string(&e).unwrap();
        assert!(!text.contains("private"));
        assert!(!text.contains("other.com"));
        let json = serde_json::to_string(&g).unwrap();
        assert!(!json.contains("password"));
        assert!(!json.contains("username"));
    }
}
