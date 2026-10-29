use std::fmt;

// HashAlgorithm is used to indicate the hash algorithm used
// https://www.iana.org/assignments/tls-parameters/tls-parameters.xhtml#tls-parameters-18
// Supported hash hash algorithms
#[derive(Copy, Clone, Debug, PartialEq)]
pub(crate) enum HashAlgorithm {
    Md2 = 0,  // Blacklisted
    Md5 = 1,  // Blacklisted
    Sha1 = 2, // Blacklisted
    Sha224 = 3,
    Sha256 = 4,
    Sha384 = 5,
    Sha512 = 6,
    Ed25519 = 8,
    Unsupported,
}

impl From<u8> for HashAlgorithm {
    fn from(val: u8) -> Self {
        match val {
            0 => HashAlgorithm::Md2,
            1 => HashAlgorithm::Md5,
            2 => HashAlgorithm::Sha1,
            3 => HashAlgorithm::Sha224,
            4 => HashAlgorithm::Sha256,
            5 => HashAlgorithm::Sha384,
            6 => HashAlgorithm::Sha512,
            8 => HashAlgorithm::Ed25519,
            _ => HashAlgorithm::Unsupported,
        }
    }
}

impl fmt::Display for HashAlgorithm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            HashAlgorithm::Md2 => write!(f, "md2"),
            HashAlgorithm::Md5 => write!(f, "md5"), // [RFC3279]
            HashAlgorithm::Sha1 => write!(f, "sha-1"), // [RFC3279]
            HashAlgorithm::Sha224 => write!(f, "sha-224"), // [RFC4055]
            HashAlgorithm::Sha256 => write!(f, "sha-256"), // [RFC4055]
            HashAlgorithm::Sha384 => write!(f, "sha-384"), // [RFC4055]
            HashAlgorithm::Sha512 => write!(f, "sha-512"), // [RFC4055]
            HashAlgorithm::Ed25519 => write!(f, "null"), // [RFC4055]
            _ => write!(f, "unknown or unsupported hash algorithm"),
        }
    }
}

// https://www.iana.org/assignments/tls-parameters/tls-parameters.xhtml#tls-parameters-16
#[derive(Copy, Clone, Debug, PartialEq)]
pub(crate) enum SignatureAlgorithm {
    Rsa = 1,
    Ecdsa = 3,
    Ed25519 = 7,
    Unsupported,
}

impl From<u8> for SignatureAlgorithm {
    fn from(val: u8) -> Self {
        match val {
            1 => SignatureAlgorithm::Rsa,
            3 => SignatureAlgorithm::Ecdsa,
            7 => SignatureAlgorithm::Ed25519,
            _ => SignatureAlgorithm::Unsupported,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub(crate) struct SignatureHashAlgorithm {
    pub(crate) hash: HashAlgorithm,
    pub(crate) signature: SignatureAlgorithm,
}

impl SignatureHashAlgorithm {}

pub(crate) fn default_signature_schemes() -> Vec<SignatureHashAlgorithm> {
    vec![
        SignatureHashAlgorithm {
            hash: HashAlgorithm::Sha256,
            signature: SignatureAlgorithm::Ecdsa,
        },
        SignatureHashAlgorithm {
            hash: HashAlgorithm::Sha384,
            signature: SignatureAlgorithm::Ecdsa,
        },
        SignatureHashAlgorithm {
            hash: HashAlgorithm::Sha512,
            signature: SignatureAlgorithm::Ecdsa,
        },
        SignatureHashAlgorithm {
            hash: HashAlgorithm::Sha256,
            signature: SignatureAlgorithm::Rsa,
        },
        SignatureHashAlgorithm {
            hash: HashAlgorithm::Sha384,
            signature: SignatureAlgorithm::Rsa,
        },
        SignatureHashAlgorithm {
            hash: HashAlgorithm::Sha512,
            signature: SignatureAlgorithm::Rsa,
        },
        SignatureHashAlgorithm {
            hash: HashAlgorithm::Ed25519,
            signature: SignatureAlgorithm::Ed25519,
        },
    ]
}

// select Signature Scheme returns most preferred and compatible scheme.

// // SignatureScheme identifies a signature algorithm supported by TLS. See
// // RFC 8446, Section 4.2.3.
// #[derive(Copy, Clone, Debug, PartialEq)]
// pub(crate) enum SignatureScheme {}
