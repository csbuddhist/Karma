use std::{fs, path::Path};

use rand::{RngCore, rngs::OsRng};
use windows::{
    Win32::{
        Foundation::{HLOCAL, LocalFree},
        Security::Cryptography::{
            CRYPT_INTEGER_BLOB, CRYPTPROTECT_LOCAL_MACHINE, CryptProtectData, CryptUnprotectData,
        },
    },
    core::w,
};
use zeroize::Zeroize;

const KEY_BYTES: usize = 32;
const MAX_PROTECTED_KEY_BYTES: usize = 4096;

pub fn load_or_create(path: &Path) -> Result<[u8; KEY_BYTES], ()> {
    if path.is_file() {
        return unprotect(&fs::read(path).map_err(|_| ())?);
    }
    let mut key = [0_u8; KEY_BYTES];
    OsRng.fill_bytes(&mut key);
    let protected = protect(&key)?;
    fs::write(path, protected).map_err(|_| ())?;
    Ok(key)
}

fn protect(key: &[u8; KEY_BYTES]) -> Result<Vec<u8>, ()> {
    let input = CRYPT_INTEGER_BLOB {
        cbData: KEY_BYTES as u32,
        pbData: key.as_ptr().cast_mut(),
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    unsafe {
        CryptProtectData(
            &input,
            w!("Karma evidence master key"),
            None,
            None,
            None,
            CRYPTPROTECT_LOCAL_MACHINE,
            &mut output,
        )
    }
    .map_err(|_| ())?;
    copy_and_free(output)
}

fn unprotect(protected: &[u8]) -> Result<[u8; KEY_BYTES], ()> {
    if protected.is_empty() || protected.len() > MAX_PROTECTED_KEY_BYTES {
        return Err(());
    }
    let input = CRYPT_INTEGER_BLOB {
        cbData: protected.len() as u32,
        pbData: protected.as_ptr().cast_mut(),
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    unsafe { CryptUnprotectData(&input, None, None, None, None, 0, &mut output) }
        .map_err(|_| ())?;
    let mut plain = copy_and_free(output)?;
    if plain.len() != KEY_BYTES {
        plain.zeroize();
        return Err(());
    }
    let mut key = [0_u8; KEY_BYTES];
    key.copy_from_slice(&plain);
    plain.zeroize();
    Ok(key)
}

fn copy_and_free(blob: CRYPT_INTEGER_BLOB) -> Result<Vec<u8>, ()> {
    if blob.pbData.is_null() || blob.cbData == 0 || blob.cbData as usize > MAX_PROTECTED_KEY_BYTES {
        if !blob.pbData.is_null() {
            unsafe {
                let _ = LocalFree(Some(HLOCAL(blob.pbData.cast())));
            }
        }
        return Err(());
    }
    let value = unsafe { std::slice::from_raw_parts(blob.pbData, blob.cbData as usize) }.to_vec();
    unsafe {
        let _ = LocalFree(Some(HLOCAL(blob.pbData.cast())));
    }
    Ok(value)
}
