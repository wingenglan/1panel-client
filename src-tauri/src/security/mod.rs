use secrecy::{ExposeSecret, SecretString};

const REDACTED: &str = "[REDACTED]";

pub fn redact(input: &str) -> String {
    let mut output = input.to_string();
    for marker in [
        "password=",
        "password:",
        "passphrase=",
        "token=",
        "authorization:",
        "private key",
    ] {
        output = redact_after_marker(&output, marker);
    }
    output
}

/// Quotes one argument for a POSIX shell. Callers must still keep the program
/// name and command shape fixed inside the Rust domain service.
#[allow(dead_code)]
pub fn shell_escape(argument: &str) -> String {
    if argument.is_empty() {
        return "''".to_string();
    }
    format!("'{}'", argument.replace('\'', "'\"'\"'"))
}

fn redact_after_marker(input: &str, marker: &str) -> String {
    let lowercase = input.to_ascii_lowercase();
    let Some(start) = lowercase.find(marker) else {
        return input.to_string();
    };
    let value_start = start + marker.len();
    let value_end = input[value_start..]
        .find(|character: char| character.is_whitespace() || character == ',' || character == ';')
        .map(|offset| value_start + offset)
        .unwrap_or(input.len());
    format!(
        "{}{}{}",
        &input[..value_start],
        REDACTED,
        &input[value_end..]
    )
}

pub trait CredentialStore: Send + Sync {
    fn put(&self, reference: &str, secret: SecretString) -> crate::errors::AppResult<()>;
    fn get(&self, reference: &str) -> crate::errors::AppResult<SecretString>;
    fn delete(&self, reference: &str) -> crate::errors::AppResult<()>;
}

pub struct OsCredentialStore {
    service: String,
}

impl OsCredentialStore {
    pub fn new(service: impl Into<String>) -> Self {
        Self {
            service: service.into(),
        }
    }

    fn entry(&self, reference: &str) -> crate::errors::AppResult<keyring::Entry> {
        keyring::Entry::new(&self.service, reference).map_err(crate::errors::AppError::credential)
    }
}

impl CredentialStore for OsCredentialStore {
    fn put(&self, reference: &str, secret: SecretString) -> crate::errors::AppResult<()> {
        self.entry(reference)?
            .set_password(secret.expose_secret())
            .map_err(crate::errors::AppError::credential)
    }

    fn get(&self, reference: &str) -> crate::errors::AppResult<SecretString> {
        self.entry(reference)?
            .get_password()
            .map(SecretString::from)
            .map_err(crate::errors::AppError::credential)
    }

    fn delete(&self, reference: &str) -> crate::errors::AppResult<()> {
        match self.entry(reference)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(crate::errors::AppError::credential(error)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{redact, shell_escape};

    #[test]
    fn redacts_common_secret_assignments() {
        let value = redact("password=hunter2 token=abc123 user=root");
        assert_eq!(value, "password=[REDACTED] token=[REDACTED] user=root");
    }

    #[test]
    fn escapes_posix_shell_arguments_without_interpolation() {
        assert_eq!(shell_escape("hello world"), "'hello world'");
        assert_eq!(shell_escape("a'b;$HOME"), "'a'\"'\"'b;$HOME'");
        assert_eq!(shell_escape(""), "''");
    }
}
