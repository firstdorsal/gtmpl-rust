//! Mows-specific cryptographic functions

use crate::{FuncError, Value};
use core::str;
use sha1::Sha1;
use sha2::{Digest, Sha256, Sha512};

pub fn mows_digest(args: &[Value]) -> Result<Value, FuncError> {
    let method = &args.first().ok_or(FuncError::ExactlyXArgs(
        "This function requires exactly 2 arguments.".to_string(),
        2,
    ))?;
    let method = method.to_string();

    let to_be_hashed = &args.get(1).ok_or(FuncError::ExactlyXArgs(
        "This function requires exactly 2 arguments.".to_string(),
        2,
    ))?;
    let to_be_hashed = to_be_hashed.to_string();
    let to_be_hashed = to_be_hashed.as_bytes();

    let hash = match method.as_str() {
        "MD5" => {
            let digest = md5::compute(to_be_hashed).to_ascii_lowercase();
            str::from_utf8(&digest)
                .map_err(|_| FuncError::Generic("Invalid hash".to_string()))?
                .to_string()
        }
        "SHA1" => {
            let digest = Sha1::digest(to_be_hashed);
            str::from_utf8(digest.as_slice())
                .map_err(|_| FuncError::Generic("Invalid hash".to_string()))?
                .to_string()
        }
        "SHA256" => {
            let digest = Sha256::digest(to_be_hashed);
            str::from_utf8(digest.as_slice())
                .map_err(|_| FuncError::Generic("Invalid hash".to_string()))?
                .to_string()
        }
        "SHA512" => {
            let digest = Sha512::digest(to_be_hashed);
            str::from_utf8(digest.as_slice())
                .map_err(|_| FuncError::Generic("Invalid hash".to_string()))?
                .to_string()
        }
        _ => return Err(FuncError::Generic("Invalid hash method".to_string())),
    };

    Ok(Value::from(hash))
}

pub fn random_string(args: &[Value]) -> Result<Value, FuncError> {
    let method = &args.first().ok_or(FuncError::ExactlyXArgs(
        "This function requires exactly 2 arguments.".to_string(),
        2,
    ))?;
    let method = method.to_string();

    let length = args.get(1).ok_or(FuncError::ExactlyXArgs(
        "This function requires exactly 2 arguments.".to_string(),
        2,
    ))?;

    let length: u16 = length.to_string().replace(' ', "").parse().map_err(|_| {
        FuncError::Generic("randomString: length must be an integer in 0..=65535".to_string())
    })?;

    let mut charset: Vec<u8> = b"".to_vec();
    if method.contains('A') {
        charset.extend(b"ABCDEFGHIJKLMNOPQRSTUVWXYZ");
    }
    if method.contains('a') {
        charset.extend(b"abcdefghijklmnopqrstuvwxyz");
    }
    if method.contains('0') {
        charset.extend(b"0123456789");
    }
    if method.contains('%') {
        charset.extend(b"%!@#$%^&*()_+-=[]{}|;':,./<>?`~");
    }
    // `rng.random_range(0..0)` panics on an empty range, so reject an empty
    // charset explicitly instead of crashing.
    if charset.is_empty() {
        return Err(FuncError::Generic(
            "randomString: method must contain at least one of 'A', 'a', '0', '%'".to_string(),
        ));
    }
    let mut rng = rand::rng();

    let generated: String = (0..length)
        .map(|_| {
            let idx = rand::Rng::random_range(&mut rng, 0..charset.len());
            charset[idx] as char
        })
        .collect();

    Ok(Value::from(generated))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_random_string_alpha() {
        let result =
            random_string(&[Value::String("Aa".to_string()), Value::Number(10.into())]).unwrap();
        let result_str = result.to_string();

        assert_eq!(result_str.len(), 10);
        assert!(result_str.chars().all(|c| c.is_alphabetic()));
    }

    #[test]
    fn test_random_string_numeric() {
        let result =
            random_string(&[Value::String("0".to_string()), Value::Number(10.into())]).unwrap();
        let result_str = result.to_string();

        assert_eq!(result_str.len(), 10);
        assert!(result_str.chars().all(|c| c.is_numeric()));
    }
}
