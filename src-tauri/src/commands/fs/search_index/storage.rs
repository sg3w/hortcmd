//! Optional persistence: per-scope passphrase, authenticated encryption, private files.
use super::{bad, types::Snapshot, MAX_BYTES};
use crate::errors::AppResult;
use ring::{aead, pbkdf2};
use std::{
    fs,
    io::{Read, Write},
    num::NonZeroU32,
    path::{Path, PathBuf},
};

pub struct Secret {
    pub salt: [u8; 16],
    pub key: [u8; 32],
}
impl Drop for Secret {
    fn drop(&mut self) {
        self.key.fill(0);
    }
}
fn derive(password: &str, salt: [u8; 16]) -> AppResult<Secret> {
    if password.len() < 12 || password.len() > 1024 {
        return Err(bad("index passphrase must contain 12 to 1024 bytes"));
    }
    let mut secret = Secret { salt, key: [0; 32] };
    pbkdf2::derive(
        pbkdf2::PBKDF2_HMAC_SHA256,
        NonZeroU32::new(210_000).unwrap(),
        &salt,
        password.as_bytes(),
        &mut secret.key,
    );
    Ok(secret)
}
pub fn secret(password: &str) -> AppResult<Secret> {
    let mut salt = [0; 16];
    getrandom::fill(&mut salt).map_err(|_| bad("randomness unavailable"))?;
    derive(password, salt)
}
fn cipher(secret: &Secret) -> AppResult<aead::LessSafeKey> {
    Ok(aead::LessSafeKey::new(
        aead::UnboundKey::new(&aead::AES_256_GCM, &secret.key)
            .map_err(|_| bad("index encryption unavailable"))?,
    ))
}
pub fn encode(snapshot: &Snapshot, secret: &Secret) -> AppResult<Vec<u8>> {
    let mut data = serde_json::to_vec(snapshot).map_err(|_| bad("index serialization failed"))?;
    if data.len() > MAX_BYTES {
        return Err(bad("index storage limit"));
    }
    let mut nonce = [0; 12];
    getrandom::fill(&mut nonce).map_err(|_| bad("randomness unavailable"))?;
    cipher(secret)?
        .seal_in_place_append_tag(
            aead::Nonce::assume_unique_for_key(nonce),
            aead::Aad::from(snapshot.id.as_bytes()),
            &mut data,
        )
        .map_err(|_| bad("index encryption failed"))?;
    let mut result = b"HIDX1".to_vec();
    result.extend(secret.salt);
    result.extend(nonce);
    result.extend(data);
    Ok(result)
}
pub fn decode(id: &str, bytes: &[u8], password: &str) -> AppResult<(Snapshot, Secret)> {
    if bytes.len() < 49 || bytes.len() > MAX_BYTES + 49 || &bytes[..5] != b"HIDX1" {
        return Err(bad("index is corrupt or outdated; delete and rebuild"));
    }
    let secret = derive(
        password,
        bytes[5..21].try_into().map_err(|_| bad("index salt"))?,
    )?;
    let nonce = bytes[21..33].try_into().map_err(|_| bad("index nonce"))?;
    let mut data = bytes[33..].to_vec();
    let plain = cipher(&secret)?
        .open_in_place(
            aead::Nonce::assume_unique_for_key(nonce),
            aead::Aad::from(id.as_bytes()),
            &mut data,
        )
        .map_err(|_| bad("incorrect passphrase or corrupt index; unlock or rebuild"))?;
    let snapshot: Snapshot =
        serde_json::from_slice(plain).map_err(|_| bad("index schema changed; rebuild"))?;
    if snapshot.version != 1 || snapshot.id != id {
        return Err(bad("index schema changed; rebuild"));
    }
    super::validate_snapshot(&snapshot)?;
    Ok((snapshot, secret))
}
pub fn directory() -> AppResult<PathBuf> {
    let path = crate::config::path("search-index")?;
    private_directory(&path)?;
    Ok(path)
}
pub(super) fn private_directory(path: &Path) -> AppResult<()> {
    if let Ok(meta) = fs::symlink_metadata(path) {
        if !meta.is_dir() || meta.file_type().is_symlink() {
            return Err(bad("unsafe index directory"));
        }
    }
    fs::create_dir_all(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}
pub fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 120 && id.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-')
}
pub fn path(id: &str) -> AppResult<PathBuf> {
    if !valid_id(id) {
        return Err(bad("invalid index id"));
    }
    Ok(directory()?.join(format!("{id}.enc")))
}
pub fn write(snapshot: &Snapshot, secret: &Secret) -> AppResult<()> {
    write_to(snapshot, secret, &path(&snapshot.id)?)
}
pub(super) fn write_to(snapshot: &Snapshot, secret: &Secret, destination: &Path) -> AppResult<()> {
    let temporary = destination.with_extension(format!("{}.tmp", crate::net::unique_id()));
    let data = encode(snapshot, secret)?;
    let mut options = fs::OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let result = (|| -> AppResult<()> {
        let mut file = options.open(&temporary)?;
        file.write_all(&data)?;
        file.sync_all()?;
        fs::rename(&temporary, destination)?;
        fs::File::open(destination.parent().unwrap())?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}
pub fn read(id: &str, password: &str) -> AppResult<(Snapshot, Secret)> {
    let path = path(id)?;
    if fs::symlink_metadata(&path)?.file_type().is_symlink() {
        return Err(bad("unsafe index file"));
    }
    let mut data = Vec::new();
    fs::File::open(path)?
        .take((MAX_BYTES + 50) as u64)
        .read_to_end(&mut data)?;
    decode(id, &data, password)
}
pub fn remove(id: &str) -> AppResult<()> {
    match fs::remove_file(path(id)?) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}
pub fn ids_in(path: &Path) -> AppResult<Vec<String>> {
    let mut ids = Vec::new();
    for entry in fs::read_dir(path)?.take(129) {
        let entry = entry?;
        if let Some(name) = entry
            .file_name()
            .to_str()
            .and_then(|s| s.strip_suffix(".enc"))
        {
            if valid_id(name) {
                ids.push(name.to_owned());
            }
        }
    }
    Ok(ids)
}
