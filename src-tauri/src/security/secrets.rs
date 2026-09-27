use crate::error::{AppError, Result};
use zeroize::Zeroizing;
pub trait SecretStore: Send + Sync {
    fn get(&self) -> Result<Option<Zeroizing<String>>>;
    fn set(&self, value: &str) -> Result<()>;
    fn delete(&self) -> Result<()>;
}
pub struct OsSecretStore;
impl OsSecretStore {
    fn entry() -> Result<keyring::Entry> {
        keyring::Entry::new("com.folio.expenses", "vision-api-key").map_err(|_| {
            AppError::new(
                "CredentialError",
                "The operating system credential store is unavailable.",
            )
        })
    }
}
impl SecretStore for OsSecretStore {
    fn get(&self) -> Result<Option<Zeroizing<String>>> {
        Self::entry()?.get()
    }
    fn set(&self, value: &str) -> Result<()> {
        Self::entry()?.set(value)
    }
    fn delete(&self) -> Result<()> {
        Self::entry()?.delete()
    }
}
// Shared implementation lets explicit platform checks exercise the exact production
// credential operations with a disposable entry, without replacing the user's key.
impl SecretStore for keyring::Entry {
    fn get(&self) -> Result<Option<Zeroizing<String>>> {
        match self.get_password() {
            Ok(v) => Ok(Some(Zeroizing::new(v))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(_) => Err(AppError::new(
                "CredentialError",
                "Unable to read the OS credential store. Offline extraction remains available.",
            )),
        }
    }
    fn set(&self, value: &str) -> Result<()> {
        self.set_password(value).map_err(|_| {
            AppError::new(
                "CredentialError",
                "Unable to save the credential in the OS credential store.",
            )
        })
    }
    fn delete(&self) -> Result<()> {
        match self.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err(AppError::new(
                "CredentialError",
                "Unable to delete the OS credential.",
            )),
        }
    }
}

#[cfg(feature = "test-support")]
pub fn verification_store(id: &str) -> Result<impl SecretStore> {
    uuid::Uuid::parse_str(id).map_err(|_| AppError::invalid("Invalid verification ID."))?;
    keyring::Entry::new("com.folio.expenses.verification", id).map_err(|_| {
        AppError::new(
            "CredentialError",
            "Cannot open the verification vault entry.",
        )
    })
}
