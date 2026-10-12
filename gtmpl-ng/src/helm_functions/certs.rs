use crate::{FuncError, Value};
use ::time::{Duration, OffsetDateTime};
use rcgen::{
    BasicConstraints, Certificate, CertificateParams, DnType, ExtendedKeyUsagePurpose, IsCa,
    KeyPair, KeyUsagePurpose,
};
use std::collections::HashMap;
use x509_parser::certificate::X509Certificate;
use x509_parser::prelude::FromDer;

pub fn gen_ca(args: &[Value]) -> Result<Value, FuncError> {
    let common_name = &args.first().ok_or(FuncError::ExactlyXArgs(
        "This function requires exactly 1 argument.".to_string(),
        1,
    ))?;
    let common_name = common_name.to_string();

    let days = args.get(1).ok_or(FuncError::ExactlyXArgs(
        "This function requires exactly 2 arguments.".to_string(),
        2,
    ))?;

    let days = days.to_string();

    let (ca_cert, ca_keys) = new_ca(&common_name, &days)?;

    let mut result = HashMap::new();

    result.insert("Cert".to_string(), Value::from(ca_cert.pem()));
    result.insert("Key".to_string(), Value::from(ca_keys.serialize_pem()));

    Ok(crate::Value::Map(result))
}

fn new_ca(common_name: &str, days: &str) -> anyhow::Result<(Certificate, KeyPair)> {
    let mut params =
        CertificateParams::new(Vec::default()).expect("empty subject alt name can't produce error");
    let (from, until) = validity_period(days.parse::<i64>()?, "genCA")?;
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    params
        .distinguished_name
        .push(DnType::CommonName, common_name.to_string());
    params
        .distinguished_name
        .push(DnType::OrganizationName, common_name.to_string());
    params.key_usages.push(KeyUsagePurpose::DigitalSignature);
    params.key_usages.push(KeyUsagePurpose::KeyCertSign);
    params.key_usages.push(KeyUsagePurpose::CrlSign);

    params.not_before = from;
    params.not_after = until;

    let key_pair = KeyPair::generate()?;

    Ok((params.self_signed(&key_pair)?, key_pair))
}

/// Today, and today plus `days`, refusing a span that does not fit a date.
///
/// A template names the lifetime, so it can name any number, and two separate steps
/// overflow before `OffsetDateTime`'s own range does: `Duration::days` multiplies by
/// 86 400 and panics if that leaves `i64`, and adding the result to today leaves the
/// supported year range long before. Both were unchecked -- `{{ genCA "x" 999999999 }}`
/// panicked, out of a template literal, past the `Result` these functions return.
fn validity_period(
    days: i64,
    function: &str,
) -> Result<(OffsetDateTime, OffsetDateTime), FuncError> {
    let too_long = || FuncError::InvalidValidity {
        function: function.to_owned(),
        days,
    };
    // Seconds are the unit `Duration` stores, so this constructor cannot overflow --
    // unlike `Duration::days`, which does the same multiplication and panics.
    let seconds = days.checked_mul(86_400).ok_or_else(too_long)?;
    let now = OffsetDateTime::now_utc();
    let until = now
        .checked_add(Duration::seconds(seconds))
        .ok_or_else(too_long)?;
    // `OffsetDateTime` reaches further than an X.509 date: the ASN.1 writer asserts
    // the year is within 0..=9999 and panics outside it, so a validity that lands
    // there -- `{{ genCA "x" -800000 }}` is year -164 -- has to be refused here.
    if !(0..=9999).contains(&until.year()) {
        return Err(too_long());
    }
    Ok((now, until))
}

/// Parse a CA certificate from PEM and rebuild it with rcgen for signing
fn parse_ca_cert_and_rebuild(
    ca_cert_pem: &str,
    ca_key_pair: &KeyPair,
) -> Result<Certificate, FuncError> {
    // Decode PEM to get DER bytes
    let pem = ::pem::parse(ca_cert_pem)
        .map_err(|e| FuncError::Generic(format!("Error parsing CA PEM: {}", e)))?;

    // Parse the X.509 certificate to extract information
    let (_, x509_cert) = X509Certificate::from_der(pem.contents())
        .map_err(|e| FuncError::Generic(format!("Error parsing CA certificate: {}", e)))?;

    // Extract common name from subject
    let cn: &str = x509_cert
        .subject()
        .iter_common_name()
        .next()
        .and_then(|attr| attr.as_str().ok())
        .unwrap_or("CA");

    // Rebuild CA params
    let mut ca_params =
        CertificateParams::new(Vec::default()).expect("empty subject alt name can't produce error");
    ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    ca_params
        .distinguished_name
        .push(DnType::CommonName, cn.to_string());
    ca_params
        .distinguished_name
        .push(DnType::OrganizationName, cn.to_string());
    ca_params.key_usages.push(KeyUsagePurpose::DigitalSignature);
    ca_params.key_usages.push(KeyUsagePurpose::KeyCertSign);
    ca_params.key_usages.push(KeyUsagePurpose::CrlSign);

    // Use the original validity period from the parsed cert
    ca_params.not_before =
        OffsetDateTime::from_unix_timestamp(x509_cert.validity().not_before.timestamp())
            .unwrap_or_else(|_| OffsetDateTime::now_utc());
    ca_params.not_after =
        OffsetDateTime::from_unix_timestamp(x509_cert.validity().not_after.timestamp())
            .unwrap_or_else(|_| OffsetDateTime::now_utc());

    // Re-sign to get a Certificate object
    ca_params
        .self_signed(ca_key_pair)
        .map_err(|e| FuncError::Generic(format!("Error rebuilding CA certificate: {}", e)))
}

pub fn gen_signed_cert(args: &[Value]) -> Result<Value, FuncError> {
    let common_name = &args.first().ok_or(FuncError::ExactlyXArgs(
        "This function requires exactly 5 arguments.".to_string(),
        5,
    ))?;
    let common_name = common_name.to_string();

    let ip_list = &args.get(1).ok_or(FuncError::ExactlyXArgs(
        "This function requires exactly 5 arguments.".to_string(),
        5,
    ))?;

    let ip_list = match ip_list {
        Value::Array(ips) => {
            let ips = ips.iter().map(|v| v.to_string()).collect::<Vec<String>>();
            Some(ips)
        }
        _ => None,
    };

    let dns_names = &args.get(2).ok_or(FuncError::ExactlyXArgs(
        "This function requires exactly 5 arguments.".to_string(),
        5,
    ))?;

    let dns_names = match dns_names {
        Value::Array(dns_names) => {
            let dns_names = dns_names
                .iter()
                .map(|v| v.to_string())
                .collect::<Vec<String>>();
            Some(dns_names)
        }
        _ => None,
    };

    // concatenate the ips and dns_names into a string vec
    let alt_names = match (ip_list, dns_names) {
        (Some(ips), Some(mut dns_names)) => {
            let mut alt_names = ips;
            alt_names.append(&mut dns_names);
            alt_names
        }
        (Some(ips), None) => ips,
        (None, Some(dns_names)) => dns_names,
        (None, None) => vec![],
    };

    let days = &args.get(3).ok_or(FuncError::ExactlyXArgs(
        "This function requires exactly 5 arguments.".to_string(),
        5,
    ))?;

    let days = days.to_string();

    let days = days
        .parse::<u32>()
        .map_err(|_| FuncError::Generic("Invalid number".to_string()))?;

    let ca = &args.get(4).ok_or(FuncError::ExactlyXArgs(
        "This function requires exactly 5 arguments.".to_string(),
        5,
    ))?;

    let (ca_cert_pem, ca_key_pair_pem) = match ca {
        Value::Map(ca) => {
            let cert = ca
                .get("Cert")
                .ok_or(FuncError::Generic("No cert found".to_string()))?;
            let key = ca
                .get("Key")
                .ok_or(FuncError::Generic("No key found".to_string()))?;

            let cert = cert.to_string();
            let key = key.to_string();

            (cert, key)
        }
        _ => return Err(FuncError::Generic("Invalid CA".to_string())),
    };

    let mut own_cert_params: CertificateParams = rcgen::CertificateParams::new(alt_names)
        .map_err(|e| FuncError::Generic(format!("Error generating certificate params: {}", e)))?;

    own_cert_params.not_after = validity_period(days.into(), "genSignedCert")?.1;
    own_cert_params
        .distinguished_name
        .push(DnType::CommonName, common_name.clone());
    own_cert_params.use_authority_key_identifier_extension = true;
    own_cert_params
        .key_usages
        .push(KeyUsagePurpose::DigitalSignature);
    own_cert_params
        .extended_key_usages
        .push(ExtendedKeyUsagePurpose::ServerAuth);

    let ca_key_pair = rcgen::KeyPair::from_pem(&ca_key_pair_pem)
        .map_err(|e| FuncError::Generic(format!("Error generating key pair: {}", e)))?;

    // Parse the CA certificate PEM to extract information
    let ca_cert = parse_ca_cert_and_rebuild(&ca_cert_pem, &ca_key_pair)?;

    let own_key_pair = rcgen::KeyPair::generate()
        .map_err(|e| FuncError::Generic(format!("Error generating key pair: {}", e)))?;

    let cert = own_cert_params
        .signed_by(&own_key_pair, &ca_cert, &ca_key_pair)
        .map_err(|e| FuncError::Generic(format!("Error generating certificate: {}", e)))?;

    let mut result = HashMap::new();

    result.insert("Cert".to_string(), Value::from(cert.pem()));
    result.insert("Key".to_string(), Value::from(own_key_pair.serialize_pem()));

    Ok(crate::Value::Map(result))
}

pub fn gen_self_signed_cert(args: &[Value]) -> Result<Value, FuncError> {
    let common_name = &args.first().ok_or(FuncError::ExactlyXArgs(
        "This function requires exactly 4 arguments.".to_string(),
        4,
    ))?;

    let common_name = common_name.to_string();

    let ips = &args.get(1).unwrap_or(&Value::Nil);

    let ips = match ips {
        Value::Array(ips) => {
            let ips = ips.iter().map(|v| v.to_string()).collect::<Vec<String>>();
            Some(ips)
        }
        _ => None,
    };

    let dns_names = &args.get(2).unwrap_or(&Value::Nil);

    let dns_names = match dns_names {
        Value::Array(dns_names) => {
            let dns_names = dns_names
                .iter()
                .map(|v| v.to_string())
                .collect::<Vec<String>>();
            Some(dns_names)
        }
        _ => None,
    };

    // concatenate the ips and dns_names into a string vec
    let alt_names = match (ips, dns_names) {
        (Some(ips), Some(mut dns_names)) => {
            let mut alt_names = ips;
            alt_names.append(&mut dns_names);
            alt_names
        }
        (Some(ips), None) => ips,
        (None, Some(dns_names)) => dns_names,
        (None, None) => vec![],
    };

    let days = &args.get(3).ok_or(FuncError::ExactlyXArgs(
        "This function requires exactly 4 arguments.".to_string(),
        4,
    ))?;

    let days = days.to_string();

    let days = days
        .parse::<u32>()
        .map_err(|_| FuncError::Generic("Invalid number".to_string()))?;

    let mut cert_params: CertificateParams = rcgen::CertificateParams::new(alt_names)
        .map_err(|e| FuncError::Generic(format!("Error generating certificate params: {}", e)))?;

    cert_params.not_after = validity_period(days.into(), "genSelfSignedCert")?.1;

    cert_params
        .distinguished_name
        .push(DnType::CommonName, common_name.clone());

    cert_params.use_authority_key_identifier_extension = true;

    cert_params
        .key_usages
        .push(KeyUsagePurpose::DigitalSignature);

    cert_params
        .extended_key_usages
        .push(ExtendedKeyUsagePurpose::ServerAuth);

    let key_pair = rcgen::KeyPair::generate()
        .map_err(|e| FuncError::Generic(format!("Error generating key pair: {}", e)))?;

    let cert = cert_params
        .self_signed(&key_pair)
        .map_err(|e| FuncError::Generic(format!("Error generating certificate: {}", e)))?;

    let cert = cert.pem();

    let key = key_pair.serialize_pem();

    let mut result = HashMap::new();

    result.insert("Cert".to_string(), Value::from(cert));
    result.insert("Key".to_string(), Value::from(key));

    Ok(crate::Value::Map(result))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gen_self_signed_cert() {
        let result = gen_self_signed_cert(&[
            Value::String("foo.com".to_string()),
            Value::Array(vec![
                Value::String("10.0.0.1".to_string()),
                Value::String("10.0.0.2".to_string()),
            ]),
            Value::Array(vec![
                Value::String("bar.com".to_string()),
                Value::String("bat.com".to_string()),
            ]),
            Value::from(365),
        ])
        .unwrap();

        match result {
            Value::Map(map) => {
                let cert = map.get("Cert").unwrap().to_string();
                let key = map.get("Key").unwrap().to_string();
                assert!(cert.starts_with("-----BEGIN CERTIFICATE-----"));
                assert!(key.starts_with("-----BEGIN PRIVATE KEY-----"));
            }
            _ => panic!("Expected a map"),
        }
    }

    #[test]
    fn test_gen_ca() {
        let result = gen_ca(&[Value::String("foo.com".to_string()), Value::from(365)]).unwrap();

        match result {
            Value::Map(map) => {
                let cert = map.get("Cert").unwrap().to_string();
                let key = map.get("Key").unwrap().to_string();
                assert!(cert.starts_with("-----BEGIN CERTIFICATE-----"));
                assert!(key.starts_with("-----BEGIN PRIVATE KEY-----"));
            }
            _ => panic!("Expected a map"),
        }
    }

    #[test]
    fn test_gen_signed_cert() {
        // First generate a CA
        let ca = gen_ca(&[Value::String("test-ca".to_string()), Value::from(365)]).unwrap();

        // Then generate a signed cert
        let result = gen_signed_cert(&[
            Value::String("test.com".to_string()),
            Value::Array(vec![Value::String("127.0.0.1".to_string())]),
            Value::Array(vec![Value::String("test.com".to_string())]),
            Value::from(365),
            ca,
        ])
        .unwrap();

        match result {
            Value::Map(map) => {
                let cert = map.get("Cert").unwrap().to_string();
                let key = map.get("Key").unwrap().to_string();
                assert!(cert.starts_with("-----BEGIN CERTIFICATE-----"));
                assert!(key.starts_with("-----BEGIN PRIVATE KEY-----"));
            }
            _ => panic!("Expected a map"),
        }
    }
}
