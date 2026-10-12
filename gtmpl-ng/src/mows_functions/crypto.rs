//! Mows-specific cryptographic functions

use crate::{FuncError, Value};
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

    // Hexadecimal, as every other hash function here returns and as `sha256sum`
    // returns. This used to read the raw digest bytes as UTF-8, which they are not for
    // any practical input, so all four methods only ever returned "Invalid hash".
    let hash = match method.as_str() {
        "MD5" => format!("{:x}", md5::compute(to_be_hashed)),
        "SHA1" => format!("{:x}", Sha1::digest(to_be_hashed)),
        "SHA256" => format!("{:x}", Sha256::digest(to_be_hashed)),
        "SHA512" => format!("{:x}", Sha512::digest(to_be_hashed)),
        other => {
            return Err(FuncError::Generic(format!(
                "mowsDigest: unknown hash method {other:?}, expected MD5, SHA1, SHA256 or SHA512"
            )))
        }
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

    // The `u16` parse above already makes memory exhaustion impossible -- 65535
    // characters at worst -- but a caller who lowered the limit meant it to apply to
    // every generating function, so honour it here as `repeat` and the `rand*` family
    // do.
    if let Some(limit) = crate::limits::active().function_output {
        if usize::from(length) > limit {
            return Err(FuncError::Generic(format!(
                "randomString: {length} characters is more than the {limit} this allows"
            )));
        }
    }
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

    /// Every method returned "Invalid hash" before this: the digest bytes were read as
    /// UTF-8 instead of being hex-encoded, and nothing tested the success path.
    /// The expectations come from coreutils `md5sum`/`sha1sum`/`sha256sum`/`sha512sum`
    /// over the same input.
    #[test]
    fn digest_matches_the_reference_implementations() {
        let cases = [
            ("MD5", "5d41402abc4b2a76b9719d911017c592"),
            ("SHA1", "aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d"),
            (
                "SHA256",
                "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824",
            ),
            (
                "SHA512",
                "9b71d224bd62f3785d96d46ad3ea3d73319bfbc2890caadae2dff72519673ca7\
                 2323c3d99ba5c11d7c7acc6e14b8c5da0c4663475c2e5c3adef46f73bcdec043",
            ),
        ];
        for (method, expected) in cases {
            let hashed = mows_digest(&[Value::from(method), Value::from("hello")])
                .unwrap_or_else(|e| panic!("{method} failed: {e}"));
            assert_eq!(
                hashed.to_string(),
                expected.replace([' ', '\n'], ""),
                "{method}"
            );
        }
    }

    #[test]
    fn an_unknown_digest_method_names_the_ones_that_work() {
        let error = mows_digest(&[Value::from("sha256"), Value::from("hello")])
            .expect_err("lower case is not one of the four");
        let message = error.to_string();
        assert!(message.contains("\"sha256\""), "{message}");
        assert!(message.contains("SHA256"), "{message}");
    }

    #[test]
    fn a_digest_needs_both_arguments() {
        assert!(mows_digest(&[]).is_err());
        assert!(mows_digest(&[Value::from("SHA256")]).is_err());
    }

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
