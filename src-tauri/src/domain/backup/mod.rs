use crate::domain::server::PublicServerProfile;
use crate::errors::{AppError, AppResult};
use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use argon2::{Algorithm, Argon2, Params, Version};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

const FORMAT: &str = "1panel-client-backup";
const VERSION: u32 = 1;
const CIPHER: &str = "AES-256-GCM";
const KDF: &str = "Argon2id";
const MEMORY_KIB: u32 = 64 * 1024;
const ITERATIONS: u32 = 3;
const PARALLELISM: u32 = 1;
const KEY_LENGTH: usize = 32;
const SALT_LENGTH: usize = 16;
const NONCE_LENGTH: usize = 12;
const AAD: &[u8] = b"1panel-client-backup:v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EncryptedBackup {
    pub format: String,
    pub version: u32,
    pub cipher: String,
    pub kdf: KdfParameters,
    pub salt: String,
    pub nonce: String,
    pub ciphertext: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KdfParameters {
    pub algorithm: String,
    pub memory_kib: u32,
    pub iterations: u32,
    pub parallelism: u32,
    pub key_length: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupPayload {
    pub servers: Vec<BackupServer>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupServer {
    pub profile: PublicServerProfile,
    pub password: Option<String>,
    pub private_key_passphrase: Option<String>,
    pub sudo_password: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportBackupInput {
    pub password: SecretString,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportBackupInput {
    pub backup: String,
    pub password: SecretString,
}

/// 使用 Argon2id 和 AES-256-GCM 加密完整服务器备份；密码和 secret 仅在本次调用内存中存在。
pub fn encrypt(payload: &BackupPayload, password: &SecretString) -> AppResult<String> {
    require_password(password)?;
    let plaintext = Zeroizing::new(serde_json::to_vec(payload).map_err(|error| {
        AppError::new("BACKUP_FAILED", "backup", "无法序列化完整备份").details(error)
    })?);
    let mut salt = [0_u8; SALT_LENGTH];
    let mut nonce_bytes = [0_u8; NONCE_LENGTH];
    rand::fill(&mut salt);
    rand::fill(&mut nonce_bytes);
    let key_bytes = Zeroizing::new(derive_key(password.expose_secret().as_bytes(), &salt)?);
    let encrypted = {
        let key = Key::<Aes256Gcm>::from(*key_bytes);
        let cipher = Aes256Gcm::new(&key);
        let nonce = Nonce::from(nonce_bytes);
        cipher
            .encrypt(
                &nonce,
                aes_gcm::aead::Payload {
                    msg: &plaintext,
                    aad: AAD,
                },
            )
            .map_err(|_| AppError::new("BACKUP_FAILED", "backup", "完整备份加密失败"))?
    };
    let document = EncryptedBackup {
        format: FORMAT.into(),
        version: VERSION,
        cipher: CIPHER.into(),
        kdf: KdfParameters {
            algorithm: KDF.into(),
            memory_kib: MEMORY_KIB,
            iterations: ITERATIONS,
            parallelism: PARALLELISM,
            key_length: KEY_LENGTH as u32,
        },
        salt: STANDARD.encode(salt),
        nonce: STANDARD.encode(nonce_bytes),
        ciphertext: STANDARD.encode(encrypted),
    };
    serde_json::to_string_pretty(&document).map_err(|error| {
        AppError::new("BACKUP_FAILED", "backup", "无法输出完整备份").details(error)
    })
}

/// 校验固定 KDF 参数并解密完整服务器备份；错误不会暴露密码或密文内容。
pub fn decrypt(document: &str, password: &SecretString) -> AppResult<BackupPayload> {
    require_password(password)?;
    let encrypted: EncryptedBackup = serde_json::from_str(document)
        .map_err(|_| AppError::new("BACKUP_INVALID", "backup", "完整备份格式无效"))?;
    validate_header(&encrypted)?;
    let salt = decode_fixed::<SALT_LENGTH>(&encrypted.salt)?;
    let nonce_bytes = decode_fixed::<NONCE_LENGTH>(&encrypted.nonce)?;
    let ciphertext = STANDARD
        .decode(&encrypted.ciphertext)
        .map_err(|_| AppError::new("BACKUP_INVALID", "backup", "完整备份密文无效"))?;
    let key_bytes = Zeroizing::new(derive_key(password.expose_secret().as_bytes(), &salt)?);
    let plaintext = {
        let key = Key::<Aes256Gcm>::from(*key_bytes);
        let cipher = Aes256Gcm::new(&key);
        let nonce = Nonce::from(nonce_bytes);
        cipher
            .decrypt(
                &nonce,
                aes_gcm::aead::Payload {
                    msg: &ciphertext,
                    aad: AAD,
                },
            )
            .map_err(|_| {
                AppError::new(
                    "BACKUP_PASSWORD_INVALID",
                    "backup",
                    "备份密码错误或备份已损坏",
                )
            })?
    };
    serde_json::from_slice(&plaintext)
        .map_err(|_| AppError::new("BACKUP_INVALID", "backup", "完整备份内容无法解析"))
}

/// 判断用户是否输入了非空备份密码，避免生成不可恢复的加密文件。
fn require_password(password: &SecretString) -> AppResult<()> {
    if password.expose_secret().is_empty() {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "validation",
            "备份密码不能为空",
        ));
    }
    Ok(())
}

/// 使用固定的内存、迭代和并行参数派生 AES-256-GCM 密钥。
fn derive_key(password: &[u8], salt: &[u8]) -> AppResult<[u8; KEY_LENGTH]> {
    let params = Params::new(MEMORY_KIB, ITERATIONS, PARALLELISM, Some(KEY_LENGTH))
        .map_err(|_| AppError::new("BACKUP_FAILED", "backup", "无法初始化备份 KDF"))?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut key = [0_u8; KEY_LENGTH];
    argon
        .hash_password_into(password, salt, &mut key)
        .map_err(|_| AppError::new("BACKUP_FAILED", "backup", "无法派生备份密钥"))?;
    Ok(key)
}

/// 验证备份封套的版本和算法白名单，拒绝未经支持的参数。
fn validate_header(document: &EncryptedBackup) -> AppResult<()> {
    if document.format != FORMAT
        || document.version != VERSION
        || document.cipher != CIPHER
        || document.kdf.algorithm != KDF
        || document.kdf.memory_kib != MEMORY_KIB
        || document.kdf.iterations != ITERATIONS
        || document.kdf.parallelism != PARALLELISM
        || document.kdf.key_length != KEY_LENGTH as u32
    {
        return Err(AppError::new(
            "BACKUP_UNSUPPORTED",
            "backup",
            "不支持的完整备份版本或算法",
        ));
    }
    Ok(())
}

/// 解码并校验固定长度的二进制封套字段。
fn decode_fixed<const N: usize>(value: &str) -> AppResult<[u8; N]> {
    let decoded = STANDARD
        .decode(value)
        .map_err(|_| AppError::new("BACKUP_INVALID", "backup", "完整备份二进制字段无效"))?;
    decoded
        .try_into()
        .map_err(|_| AppError::new("BACKUP_INVALID", "backup", "完整备份二进制字段长度无效"))
}

#[cfg(test)]
mod tests {
    use super::{decrypt, encrypt, BackupPayload};
    use secrecy::SecretString;

    #[test]
    fn encrypted_backup_round_trips() {
        let payload = BackupPayload {
            servers: Vec::new(),
        };
        let password = SecretString::from("test-password");
        let document = encrypt(&payload, &password).expect("encrypt");
        assert_eq!(
            decrypt(&document, &password)
                .expect("decrypt")
                .servers
                .len(),
            0
        );
    }

    #[test]
    fn wrong_password_is_rejected() {
        let payload = BackupPayload {
            servers: Vec::new(),
        };
        let document = encrypt(&payload, &SecretString::from("correct")).expect("encrypt");
        assert!(decrypt(&document, &SecretString::from("wrong")).is_err());
    }
}
