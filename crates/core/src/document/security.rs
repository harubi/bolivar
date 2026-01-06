//! PDF Security Handlers for decryption.
//!
//! Port of pdfminer.six PDFStandardSecurityHandler classes.

use super::saslprep::saslprep;
use crate::codec::aes::{aes_cbc_decrypt, aes_cbc_encrypt, unpad_aes};
use crate::codec::arcfour::Arcfour;
use crate::model::objects::PDFObject;
use crate::{PdfError, Result};
use sha2::{Digest, Sha256, Sha384, Sha512};
use std::collections::HashMap;

/// Password padding constant from PDF spec.
pub const PASSWORD_PADDING: [u8; 32] = [
    0x28, 0xBF, 0x4E, 0x5E, 0x4E, 0x75, 0x8A, 0x41, 0x64, 0x00, 0x4E, 0x56, 0xFF, 0xFA, 0x01, 0x08,
    0x2E, 0x2E, 0x00, 0xB6, 0xD0, 0x68, 0x3E, 0x80, 0x2F, 0x0C, 0xA9, 0xFE, 0x64, 0x53, 0x69, 0x7A,
];

/// Trait for PDF security handlers.
pub trait PDFSecurityHandler: Send + Sync {
    /// Decrypt bytes for a specific object.
    ///
    /// The `attrs` parameter is used by V4+ handlers to check if the stream
    /// is metadata (which may be unencrypted per EncryptMetadata setting).
    fn decrypt(
        &self,
        objid: u32,
        genno: u16,
        data: &[u8],
        attrs: Option<&HashMap<String, PDFObject>>,
    ) -> Vec<u8>;

    /// Decrypt a string (may differ from stream decryption in V4+).
    fn decrypt_string(&self, objid: u32, genno: u16, data: &[u8]) -> Vec<u8> {
        self.decrypt(objid, genno, data, None)
    }

    /// Decrypt a stream with its attributes (may differ from string decryption in V4+).
    fn decrypt_stream(
        &self,
        objid: u32,
        genno: u16,
        data: &[u8],
        attrs: &HashMap<String, PDFObject>,
    ) -> Vec<u8> {
        self.decrypt(objid, genno, data, Some(attrs))
    }
}

/// PDF Standard Security Handler for R2 and R3 (RC4 encryption).
///
/// Supports:
/// - V=1, R=2: 40-bit RC4
/// - V=2, R=3: Variable-length RC4 (up to 128-bit)
pub struct PDFStandardSecurityHandlerV2 {
    /// The computed encryption key.
    key: Vec<u8>,
    /// Revision number (2 or 3).
    r: i64,
    /// Key length in bits.
    length: i64,
    /// Owner password hash (O value).
    o: Vec<u8>,
    /// User password hash (U value).
    u: Vec<u8>,
    /// Permission flags (P value).
    p: u32,
    /// Document ID (first element).
    docid: Vec<u8>,
}

impl PDFStandardSecurityHandlerV2 {
    /// Supported revision values.
    pub const SUPPORTED_REVISIONS: [i64; 2] = [2, 3];

    /// Create a new V2 security handler.
    ///
    /// # Arguments
    /// * `encrypt` - The /Encrypt dictionary from the PDF
    /// * `doc_id` - The document ID array
    /// * `password` - The password to try (user or owner)
    ///
    /// # Returns
    /// * `Ok(handler)` if authentication succeeds
    /// * `Err` if authentication fails or parameters are invalid
    pub fn new(
        encrypt: &HashMap<String, PDFObject>,
        doc_id: &[Vec<u8>],
        password: &str,
    ) -> Result<Self> {
        // Extract parameters from encrypt dict
        let r = get_int(encrypt, "R")?;
        let v = get_int_default(encrypt, "V", 0);
        let length = get_int_default(encrypt, "Length", 40).min(128);
        let o = get_bytes(encrypt, "O")?;
        let u = get_bytes(encrypt, "U")?;
        let p = get_uint32(encrypt, "P")?;

        // Validate revision
        if !Self::SUPPORTED_REVISIONS.contains(&r) {
            return Err(PdfError::EncryptionError(format!(
                "Unsupported revision: R={}",
                r
            )));
        }

        // Validate V matches R expectation
        if r == 2 && v != 1 {
            // R2 should use V1 but we allow V2 for compatibility
        }
        if r == 3 && v != 2 {
            // R3 should use V2
        }

        // Get document ID
        let docid = if doc_id.is_empty() {
            vec![]
        } else {
            doc_id[0].clone()
        };

        let mut handler = Self {
            key: vec![],
            r,
            length,
            o,
            u,
            p,
            docid,
        };

        // Authenticate with password
        let password_bytes = password.as_bytes();
        if let Some(key) = handler.authenticate_user_password(password_bytes) {
            handler.key = key;
            Ok(handler)
        } else if let Some(key) = handler.authenticate_owner_password(password_bytes) {
            handler.key = key;
            Ok(handler)
        } else {
            Err(PdfError::EncryptionError("Incorrect password".into()))
        }
    }

    /// Compute the encryption key from a password (Algorithm 3.2).
    fn compute_encryption_key(&self, password: &[u8]) -> Vec<u8> {
        // 1. Pad password to 32 bytes
        let mut padded = [0u8; 32];
        let len = password.len().min(32);
        padded[..len].copy_from_slice(&password[..len]);
        if len < 32 {
            padded[len..].copy_from_slice(&PASSWORD_PADDING[..32 - len]);
        }

        // 2. Initialize MD5 hash with padded password
        let mut context = md5::Context::new();
        context.consume(padded);

        // 3. Add O value
        context.consume(&self.o);

        // 4. Add P value as 4 bytes little-endian
        context.consume(self.p.to_le_bytes());

        // 5. Add document ID
        context.consume(&self.docid);

        // Get result
        let mut result = context.finalize().0.to_vec();

        // Key length in bytes
        let n = if self.r >= 3 {
            (self.length / 8) as usize
        } else {
            5 // 40-bit for R2
        };

        // For R >= 3, hash 50 more times
        if self.r >= 3 {
            for _ in 0..50 {
                let digest = md5::compute(&result[..n]);
                result = digest.0.to_vec();
            }
        }

        result[..n].to_vec()
    }

    /// Compute the U value from the key (Algorithm 3.4/3.5).
    fn compute_u_value(&self, key: &[u8]) -> Vec<u8> {
        if self.r == 2 {
            // Algorithm 3.4: Simple RC4 encryption of padding
            let mut cipher = Arcfour::new(key);
            cipher.process(&PASSWORD_PADDING)
        } else {
            // Algorithm 3.5: Hash padding + docid, then 20 RC4 iterations
            let mut context = md5::Context::new();
            context.consume(PASSWORD_PADDING);
            context.consume(&self.docid);
            let hash = context.finalize();

            let mut result = Arcfour::new(key).process(&hash.0);

            // 19 more XOR iterations (i=1 to 19)
            for i in 1..20u8 {
                let xor_key: Vec<u8> = key.iter().map(|b| b ^ i).collect();
                result = Arcfour::new(&xor_key).process(&result);
            }

            // Pad result to 32 bytes by repeating (matches pdfminer behavior)
            let mut padded = result.clone();
            padded.extend_from_slice(&result);
            padded.truncate(32);
            padded
        }
    }

    /// Verify an encryption key against the stored U value (Algorithm 3.6).
    fn verify_encryption_key(&self, key: &[u8]) -> bool {
        let computed_u = self.compute_u_value(key);
        if self.r == 2 {
            // R2: Compare full 32 bytes
            computed_u == self.u
        } else {
            // R3: Compare first 16 bytes only
            computed_u.len() >= 16 && self.u.len() >= 16 && computed_u[..16] == self.u[..16]
        }
    }

    /// Authenticate with user password.
    fn authenticate_user_password(&self, password: &[u8]) -> Option<Vec<u8>> {
        let key = self.compute_encryption_key(password);
        if self.verify_encryption_key(&key) {
            Some(key)
        } else {
            None
        }
    }

    /// Authenticate with owner password (Algorithm 3.7).
    fn authenticate_owner_password(&self, password: &[u8]) -> Option<Vec<u8>> {
        // 1. Pad password to 32 bytes
        let mut padded = [0u8; 32];
        let len = password.len().min(32);
        padded[..len].copy_from_slice(&password[..len]);
        if len < 32 {
            padded[len..].copy_from_slice(&PASSWORD_PADDING[..32 - len]);
        }

        // 2. MD5 hash of padded password
        let mut hash = md5::compute(padded).0.to_vec();

        // 3. For R >= 3, hash 50 more times
        if self.r >= 3 {
            for _ in 0..50 {
                hash = md5::compute(&hash).0.to_vec();
            }
        }

        // Key length
        let n = if self.r >= 3 {
            (self.length / 8) as usize
        } else {
            5
        };
        let key = &hash[..n];

        // 4. Decrypt O value to get user password
        let user_password = if self.r == 2 {
            // Simple RC4 decryption
            Arcfour::new(key).process(&self.o)
        } else {
            // 20 XOR iterations in reverse order (19 down to 0)
            let mut result = self.o.clone();
            for i in (0..20u8).rev() {
                let xor_key: Vec<u8> = key.iter().map(|b| b ^ i).collect();
                result = Arcfour::new(&xor_key).process(&result);
            }
            result
        };

        // 5. Try to authenticate with decrypted user password
        self.authenticate_user_password(&user_password)
    }

    /// Decrypt data using RC4 with object-specific key derivation.
    fn decrypt_rc4(&self, objid: u32, genno: u16, data: &[u8]) -> Vec<u8> {
        // Build key: base_key + objid (3 bytes LE) + genno (2 bytes LE)
        let mut key_data = self.key.clone();
        key_data.extend_from_slice(&objid.to_le_bytes()[..3]);
        key_data.extend_from_slice(&(genno as u32).to_le_bytes()[..2]);

        // Hash and truncate to min(key_length + 5, 16) bytes
        let hash = md5::compute(&key_data);
        let key_len = (self.key.len() + 5).min(16);
        let key = &hash.0[..key_len];

        Arcfour::new(key).process(data)
    }
}

impl PDFSecurityHandler for PDFStandardSecurityHandlerV2 {
    fn decrypt(
        &self,
        objid: u32,
        genno: u16,
        data: &[u8],
        _attrs: Option<&HashMap<String, PDFObject>>,
    ) -> Vec<u8> {
        self.decrypt_rc4(objid, genno, data)
    }
}

/// Crypt filter method.
#[derive(Debug, Clone, Copy, PartialEq)]
enum CryptMethod {
    Identity,
    V2,    // RC4
    AESV2, // AES-128
    AESV3, // AES-256
}

/// PDF Standard Security Handler for R4 (AES-128 encryption).
///
/// Supports V=4, R=4 with crypt filters.
pub struct PDFStandardSecurityHandlerV4 {
    /// The computed encryption key (128-bit).
    key: Vec<u8>,
    /// Revision number (4).
    #[allow(dead_code)]
    r: i64,
    /// Owner password hash (O value).
    o: Vec<u8>,
    /// User password hash (U value).
    u: Vec<u8>,
    /// Permission flags (P value).
    p: u32,
    /// Document ID (first element).
    docid: Vec<u8>,
    /// String encryption method.
    strf: CryptMethod,
    /// Stream encryption method.
    stmf: CryptMethod,
    /// Whether to encrypt metadata.
    encrypt_metadata: bool,
}

impl PDFStandardSecurityHandlerV4 {
    /// Create a new V4 security handler.
    pub fn new(
        encrypt: &HashMap<String, PDFObject>,
        doc_id: &[Vec<u8>],
        password: &str,
    ) -> Result<Self> {
        let r = get_int(encrypt, "R")?;
        if r != 4 {
            return Err(PdfError::EncryptionError(format!(
                "V4 handler requires R=4, got R={}",
                r
            )));
        }

        let o = get_bytes(encrypt, "O")?;
        let u = get_bytes(encrypt, "U")?;
        let p = get_uint32(encrypt, "P")?;

        // Get crypt filter names
        let strf_name = get_name_default(encrypt, "StrF", "Identity");
        let stmf_name = get_name_default(encrypt, "StmF", "Identity");

        // Parse crypt filter dictionary
        let cf = get_dict(encrypt, "CF").unwrap_or_default();
        let strf = Self::resolve_crypt_method(&cf, &strf_name)?;
        let stmf = Self::resolve_crypt_method(&cf, &stmf_name)?;

        let encrypt_metadata = get_bool_default(encrypt, "EncryptMetadata", true);

        let docid = if doc_id.is_empty() {
            vec![]
        } else {
            doc_id[0].clone()
        };

        let mut handler = Self {
            key: vec![],
            r,
            o,
            u,
            p,
            docid,
            strf,
            stmf,
            encrypt_metadata,
        };

        // Authenticate - reuse V2 key derivation (same algorithm for R4)
        let password_bytes = password.as_bytes();
        if let Some(key) = handler.authenticate_user_password(password_bytes) {
            handler.key = key;
            Ok(handler)
        } else if let Some(key) = handler.authenticate_owner_password(password_bytes) {
            handler.key = key;
            Ok(handler)
        } else {
            Err(PdfError::EncryptionError("Incorrect password".into()))
        }
    }

    fn resolve_crypt_method(cf: &HashMap<String, PDFObject>, name: &str) -> Result<CryptMethod> {
        if name == "Identity" {
            return Ok(CryptMethod::Identity);
        }

        let filter = cf.get(name).and_then(|v| v.as_dict().ok()).ok_or_else(|| {
            PdfError::EncryptionError(format!("Crypt filter '{}' not found in CF", name))
        })?;

        let cfm = filter
            .get("CFM")
            .and_then(|v| v.as_name().ok())
            .unwrap_or("None");

        match cfm {
            "V2" => Ok(CryptMethod::V2),
            "AESV2" => Ok(CryptMethod::AESV2),
            "AESV3" => Ok(CryptMethod::AESV3),
            "None" => Ok(CryptMethod::Identity),
            _ => Err(PdfError::EncryptionError(format!(
                "Unknown crypt filter method: {}",
                cfm
            ))),
        }
    }

    /// Compute the encryption key (Algorithm 3.2, same as V2 with 128-bit).
    fn compute_encryption_key(&self, password: &[u8]) -> Vec<u8> {
        let mut padded = [0u8; 32];
        let len = password.len().min(32);
        padded[..len].copy_from_slice(&password[..len]);
        if len < 32 {
            padded[len..].copy_from_slice(&PASSWORD_PADDING[..32 - len]);
        }

        let mut context = md5::Context::new();
        context.consume(padded);
        context.consume(&self.o);
        context.consume(self.p.to_le_bytes());
        context.consume(&self.docid);

        // For R4, if EncryptMetadata is false, add 0xFFFFFFFF
        if !self.encrypt_metadata {
            context.consume([0xFF, 0xFF, 0xFF, 0xFF]);
        }

        let mut result = context.finalize().0.to_vec();

        // Hash 50 more times for R >= 3
        for _ in 0..50 {
            let digest = md5::compute(&result[..16]);
            result = digest.0.to_vec();
        }

        result[..16].to_vec() // 128-bit key
    }

    /// Compute U value (Algorithm 3.5).
    fn compute_u_value(&self, key: &[u8]) -> Vec<u8> {
        let mut context = md5::Context::new();
        context.consume(PASSWORD_PADDING);
        context.consume(&self.docid);
        let hash = context.finalize();

        let mut result = Arcfour::new(key).process(&hash.0);

        for i in 1..20u8 {
            let xor_key: Vec<u8> = key.iter().map(|b| b ^ i).collect();
            result = Arcfour::new(&xor_key).process(&result);
        }

        let mut padded = result.clone();
        padded.extend_from_slice(&result);
        padded.truncate(32);
        padded
    }

    fn verify_encryption_key(&self, key: &[u8]) -> bool {
        let computed_u = self.compute_u_value(key);
        computed_u.len() >= 16 && self.u.len() >= 16 && computed_u[..16] == self.u[..16]
    }

    fn authenticate_user_password(&self, password: &[u8]) -> Option<Vec<u8>> {
        let key = self.compute_encryption_key(password);
        if self.verify_encryption_key(&key) {
            Some(key)
        } else {
            None
        }
    }

    fn authenticate_owner_password(&self, password: &[u8]) -> Option<Vec<u8>> {
        let mut padded = [0u8; 32];
        let len = password.len().min(32);
        padded[..len].copy_from_slice(&password[..len]);
        if len < 32 {
            padded[len..].copy_from_slice(&PASSWORD_PADDING[..32 - len]);
        }

        let mut hash = md5::compute(padded).0.to_vec();
        for _ in 0..50 {
            hash = md5::compute(&hash).0.to_vec();
        }

        let key = &hash[..16];
        let mut result = self.o.clone();
        for i in (0..20u8).rev() {
            let xor_key: Vec<u8> = key.iter().map(|b| b ^ i).collect();
            result = Arcfour::new(&xor_key).process(&result);
        }

        self.authenticate_user_password(&result)
    }

    fn decrypt_with_method(
        &self,
        method: CryptMethod,
        objid: u32,
        genno: u16,
        data: &[u8],
    ) -> Vec<u8> {
        match method {
            CryptMethod::Identity => data.to_vec(),
            CryptMethod::V2 => self.decrypt_rc4(objid, genno, data),
            CryptMethod::AESV2 => self.decrypt_aes128(objid, genno, data),
            CryptMethod::AESV3 => {
                // V4 handler shouldn't use AESV3, but handle gracefully
                data.to_vec()
            }
        }
    }

    fn decrypt_rc4(&self, objid: u32, genno: u16, data: &[u8]) -> Vec<u8> {
        let mut key_data = self.key.clone();
        key_data.extend_from_slice(&objid.to_le_bytes()[..3]);
        key_data.extend_from_slice(&(genno as u32).to_le_bytes()[..2]);

        let hash = md5::compute(&key_data);
        let key_len = (self.key.len() + 5).min(16);
        let key = &hash.0[..key_len];

        Arcfour::new(key).process(data)
    }

    fn decrypt_aes128(&self, objid: u32, genno: u16, data: &[u8]) -> Vec<u8> {
        if data.len() < 16 {
            return data.to_vec(); // Not enough data for IV
        }

        // Build key: base_key + objid (3 bytes) + genno (2 bytes) + "sAlT"
        let mut key_data = self.key.clone();
        key_data.extend_from_slice(&objid.to_le_bytes()[..3]);
        key_data.extend_from_slice(&(genno as u32).to_le_bytes()[..2]);
        key_data.extend_from_slice(b"sAlT");

        let hash = md5::compute(&key_data);
        let key = &hash.0[..16];

        let iv = &data[..16];
        let ciphertext = &data[16..];

        if ciphertext.is_empty() {
            return vec![];
        }

        let plaintext = aes_cbc_decrypt(key, iv, ciphertext);
        unpad_aes(&plaintext).to_vec()
    }

    fn is_metadata_stream(&self, attrs: Option<&HashMap<String, PDFObject>>) -> bool {
        if let Some(attrs) = attrs
            && let Some(t) = attrs.get("Type")
            && let Ok(name) = t.as_name()
        {
            return name == "Metadata";
        }
        false
    }
}

impl PDFSecurityHandler for PDFStandardSecurityHandlerV4 {
    fn decrypt(
        &self,
        objid: u32,
        genno: u16,
        data: &[u8],
        attrs: Option<&HashMap<String, PDFObject>>,
    ) -> Vec<u8> {
        // Check if we should skip metadata decryption
        if !self.encrypt_metadata && self.is_metadata_stream(attrs) {
            return data.to_vec();
        }

        // Use strf for strings (attrs=None), stmf for streams (attrs=Some)
        let method = if attrs.is_some() {
            self.stmf
        } else {
            self.strf
        };
        self.decrypt_with_method(method, objid, genno, data)
    }
}

/// PDF Standard Security Handler for R5/R6 (AES-256 encryption).
///
/// Supports V=5, R=5/6 with 256-bit AES encryption.
pub struct PDFStandardSecurityHandlerV5 {
    /// The 256-bit encryption key.
    key: Vec<u8>,
    /// Revision number (5 or 6).
    r: i64,
    /// Encrypted owner key (32 bytes).
    oe: Vec<u8>,
    /// Encrypted user key (32 bytes).
    ue: Vec<u8>,
    /// Owner hash (first 32 bytes of O).
    o_hash: Vec<u8>,
    /// Owner validation salt (bytes 32-40 of O).
    o_validation_salt: Vec<u8>,
    /// Owner key salt (bytes 40-48 of O).
    o_key_salt: Vec<u8>,
    /// User hash (first 32 bytes of U).
    u_hash: Vec<u8>,
    /// User validation salt (bytes 32-40 of U).
    u_validation_salt: Vec<u8>,
    /// User key salt (bytes 40-48 of U).
    u_key_salt: Vec<u8>,
    /// Full U value (needed for owner password verification).
    u: Vec<u8>,
    /// String encryption method.
    strf: CryptMethod,
    /// Stream encryption method.
    stmf: CryptMethod,
    /// Whether to encrypt metadata.
    encrypt_metadata: bool,
}

impl PDFStandardSecurityHandlerV5 {
    /// Supported revision values.
    pub const SUPPORTED_REVISIONS: [i64; 2] = [5, 6];

    /// Create a new V5 security handler.
    pub fn new(
        encrypt: &HashMap<String, PDFObject>,
        _doc_id: &[Vec<u8>],
        password: &str,
    ) -> Result<Self> {
        let r = get_int(encrypt, "R")?;
        if !Self::SUPPORTED_REVISIONS.contains(&r) {
            return Err(PdfError::EncryptionError(format!(
                "V5 handler requires R=5 or R=6, got R={}",
                r
            )));
        }

        let o = get_bytes(encrypt, "O")?;
        let u = get_bytes(encrypt, "U")?;
        let oe = get_bytes(encrypt, "OE")?;
        let ue = get_bytes(encrypt, "UE")?;

        // Validate lengths
        if o.len() < 48 {
            return Err(PdfError::EncryptionError(format!(
                "O value too short: {} bytes, expected 48",
                o.len()
            )));
        }
        if u.len() < 48 {
            return Err(PdfError::EncryptionError(format!(
                "U value too short: {} bytes, expected 48",
                u.len()
            )));
        }
        if oe.len() < 32 {
            return Err(PdfError::EncryptionError(format!(
                "OE value too short: {} bytes, expected 32",
                oe.len()
            )));
        }
        if ue.len() < 32 {
            return Err(PdfError::EncryptionError(format!(
                "UE value too short: {} bytes, expected 32",
                ue.len()
            )));
        }

        // Parse O and U into components
        let o_hash = o[..32].to_vec();
        let o_validation_salt = o[32..40].to_vec();
        let o_key_salt = o[40..48].to_vec();
        let u_hash = u[..32].to_vec();
        let u_validation_salt = u[32..40].to_vec();
        let u_key_salt = u[40..48].to_vec();

        // Get crypt filter names
        let strf_name = get_name_default(encrypt, "StrF", "Identity");
        let stmf_name = get_name_default(encrypt, "StmF", "Identity");

        // Parse crypt filter dictionary
        let cf = get_dict(encrypt, "CF").unwrap_or_default();
        let strf = Self::resolve_crypt_method(&cf, &strf_name)?;
        let stmf = Self::resolve_crypt_method(&cf, &stmf_name)?;

        let encrypt_metadata = get_bool_default(encrypt, "EncryptMetadata", true);

        let mut handler = Self {
            key: vec![],
            r,
            oe,
            ue,
            o_hash,
            o_validation_salt,
            o_key_salt,
            u_hash,
            u_validation_salt,
            u_key_salt,
            u,
            strf,
            stmf,
            encrypt_metadata,
        };

        // Authenticate with password
        if let Some(key) = handler.authenticate(password) {
            handler.key = key;
            Ok(handler)
        } else {
            Err(PdfError::EncryptionError("Incorrect password".into()))
        }
    }

    fn resolve_crypt_method(cf: &HashMap<String, PDFObject>, name: &str) -> Result<CryptMethod> {
        if name == "Identity" {
            return Ok(CryptMethod::Identity);
        }

        let filter = cf.get(name).and_then(|v| v.as_dict().ok()).ok_or_else(|| {
            PdfError::EncryptionError(format!("Crypt filter '{}' not found in CF", name))
        })?;

        let cfm = filter
            .get("CFM")
            .and_then(|v| v.as_name().ok())
            .unwrap_or("None");

        match cfm {
            "AESV3" => Ok(CryptMethod::AESV3),
            "AESV2" => Ok(CryptMethod::AESV2),
            "V2" => Ok(CryptMethod::V2),
            "None" => Ok(CryptMethod::Identity),
            _ => Err(PdfError::EncryptionError(format!(
                "Unknown crypt filter method: {}",
                cfm
            ))),
        }
    }

    /// Authenticate with password, trying owner password first, then user.
    fn authenticate(&self, password: &str) -> Option<Vec<u8>> {
        let password_bytes = self.normalize_password(password);

        // Try owner password first
        let hash = self.password_hash(&password_bytes, &self.o_validation_salt, Some(&self.u));
        if hash == self.o_hash {
            // Owner password validated, decrypt OE to get the key
            let key_hash = self.password_hash(&password_bytes, &self.o_key_salt, Some(&self.u));
            let key = aes_cbc_decrypt(&key_hash, &[0u8; 16], &self.oe);
            return Some(key);
        }

        // Try user password
        let hash = self.password_hash(&password_bytes, &self.u_validation_salt, None);
        if hash == self.u_hash {
            // User password validated, decrypt UE to get the key
            let key_hash = self.password_hash(&password_bytes, &self.u_key_salt, None);
            let key = aes_cbc_decrypt(&key_hash, &[0u8; 16], &self.ue);
            return Some(key);
        }

        None
    }

    /// Normalize password according to revision.
    fn normalize_password(&self, password: &str) -> Vec<u8> {
        if self.r == 6 {
            // For R6, use SASLprep then UTF-8 encode
            if password.is_empty() {
                return vec![];
            }
            let prepped = saslprep(password, true).unwrap_or_else(|_| password.to_string());
            let bytes = prepped.as_bytes();
            // Truncate to 127 bytes
            if bytes.len() > 127 {
                bytes[..127].to_vec()
            } else {
                bytes.to_vec()
            }
        } else {
            // For R5, just UTF-8 encode and truncate
            let bytes = password.as_bytes();
            if bytes.len() > 127 {
                bytes[..127].to_vec()
            } else {
                bytes.to_vec()
            }
        }
    }

    /// Compute password hash depending on revision number.
    fn password_hash(&self, password: &[u8], salt: &[u8], vector: Option<&[u8]>) -> Vec<u8> {
        if self.r == 5 {
            self.r5_password(password, salt, vector)
        } else {
            self.r6_password(password, &salt[..8], vector)
        }
    }

    /// Compute the password hash for revision 5 (simple SHA-256).
    fn r5_password(&self, password: &[u8], salt: &[u8], vector: Option<&[u8]>) -> Vec<u8> {
        let mut hasher = Sha256::new();
        hasher.update(password);
        hasher.update(salt);
        if let Some(v) = vector {
            hasher.update(v);
        }
        hasher.finalize().to_vec()
    }

    /// Compute the password hash for revision 6 (complex iterative hash).
    fn r6_password(&self, password: &[u8], salt: &[u8], vector: Option<&[u8]>) -> Vec<u8> {
        // Initial hash
        let mut hasher = Sha256::new();
        hasher.update(password);
        hasher.update(salt);
        if let Some(v) = vector {
            hasher.update(v);
        }
        let mut k = hasher.finalize().to_vec();

        let mut round_no = 0u32;
        let mut last_byte_val = 0u8;

        while round_no < 64 || last_byte_val > (round_no as u8).wrapping_sub(32) {
            // k1 = (password + k + vector) * 64
            let vector_bytes = vector.unwrap_or(&[]);
            let base: Vec<u8> = password
                .iter()
                .chain(k.iter())
                .chain(vector_bytes.iter())
                .copied()
                .collect();
            let mut k1 = Vec::with_capacity(base.len() * 64);
            for _ in 0..64 {
                k1.extend_from_slice(&base);
            }

            // AES-CBC encrypt k1 with key=k[0:16], iv=k[16:32]
            let e = aes_cbc_encrypt(&k[..16], &k[16..32], &k1);

            // Compute next hash based on first 16 bytes of e mod 3
            let hash_idx = Self::bytes_mod_3(&e[..16]);
            k = match hash_idx {
                0 => {
                    let mut h = Sha256::new();
                    h.update(&e);
                    h.finalize().to_vec()
                }
                1 => {
                    let mut h = Sha384::new();
                    h.update(&e);
                    h.finalize().to_vec()
                }
                _ => {
                    let mut h = Sha512::new();
                    h.update(&e);
                    h.finalize().to_vec()
                }
            };

            last_byte_val = e[e.len() - 1];
            round_no += 1;
        }

        k[..32].to_vec()
    }

    /// Compute sum of bytes mod 3.
    fn bytes_mod_3(input: &[u8]) -> usize {
        // 256 is 1 mod 3, so we can just sum the remainders
        input.iter().map(|&b| (b % 3) as usize).sum::<usize>() % 3
    }

    /// Decrypt data using AES-256-CBC with the base key (no object-specific derivation).
    fn decrypt_aes256(&self, data: &[u8]) -> Vec<u8> {
        if data.len() < 16 {
            return data.to_vec();
        }

        let iv = &data[..16];
        let ciphertext = &data[16..];

        if ciphertext.is_empty() {
            return vec![];
        }

        let plaintext = aes_cbc_decrypt(&self.key, iv, ciphertext);
        unpad_aes(&plaintext).to_vec()
    }

    fn decrypt_with_method(&self, method: CryptMethod, data: &[u8]) -> Vec<u8> {
        match method {
            CryptMethod::Identity => data.to_vec(),
            CryptMethod::AESV3 => self.decrypt_aes256(data),
            CryptMethod::AESV2 | CryptMethod::V2 => {
                // V5 handler should only use AESV3, but handle gracefully
                data.to_vec()
            }
        }
    }

    fn is_metadata_stream(&self, attrs: Option<&HashMap<String, PDFObject>>) -> bool {
        if let Some(attrs) = attrs
            && let Some(t) = attrs.get("Type")
            && let Ok(name) = t.as_name()
        {
            return name == "Metadata";
        }
        false
    }
}

impl PDFSecurityHandler for PDFStandardSecurityHandlerV5 {
    fn decrypt(
        &self,
        _objid: u32,
        _genno: u16,
        data: &[u8],
        attrs: Option<&HashMap<String, PDFObject>>,
    ) -> Vec<u8> {
        // Check if we should skip metadata decryption
        if !self.encrypt_metadata && self.is_metadata_stream(attrs) {
            return data.to_vec();
        }

        // Use strf for strings (attrs=None), stmf for streams (attrs=Some)
        let method = if attrs.is_some() {
            self.stmf
        } else {
            self.strf
        };
        self.decrypt_with_method(method, data)
    }
}

/// Helper: Get integer value from encrypt dict.
fn get_int(encrypt: &HashMap<String, PDFObject>, key: &str) -> Result<i64> {
    encrypt
        .get(key)
        .ok_or_else(|| PdfError::EncryptionError(format!("Missing {} in /Encrypt", key)))?
        .as_int()
}

/// Helper: Get integer value with default.
fn get_int_default(encrypt: &HashMap<String, PDFObject>, key: &str, default: i64) -> i64 {
    encrypt
        .get(key)
        .and_then(|v| v.as_int().ok())
        .unwrap_or(default)
}

/// Helper: Get bytes value from encrypt dict.
fn get_bytes(encrypt: &HashMap<String, PDFObject>, key: &str) -> Result<Vec<u8>> {
    encrypt
        .get(key)
        .ok_or_else(|| PdfError::EncryptionError(format!("Missing {} in /Encrypt", key)))?
        .as_string()
        .map(|s| s.to_vec())
}

/// Helper: Get unsigned 32-bit integer (P value handling).
fn get_uint32(encrypt: &HashMap<String, PDFObject>, key: &str) -> Result<u32> {
    let val = get_int(encrypt, key)?;
    // P can be negative in the PDF (signed interpretation), but we need unsigned
    Ok(val as u32)
}

/// Helper: Get name value with default.
fn get_name_default(encrypt: &HashMap<String, PDFObject>, key: &str, default: &str) -> String {
    encrypt
        .get(key)
        .and_then(|v| v.as_name().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| default.to_string())
}

/// Helper: Get dict value.
fn get_dict(encrypt: &HashMap<String, PDFObject>, key: &str) -> Option<HashMap<String, PDFObject>> {
    encrypt.get(key).and_then(|v| v.as_dict().ok()).cloned()
}

/// Helper: Get bool value with default.
fn get_bool_default(encrypt: &HashMap<String, PDFObject>, key: &str, default: bool) -> bool {
    encrypt
        .get(key)
        .and_then(|v| v.as_bool().ok())
        .unwrap_or(default)
}

/// Create appropriate security handler from /Encrypt dict.
pub fn create_security_handler(
    encrypt: &HashMap<String, PDFObject>,
    doc_id: &[Vec<u8>],
    password: &str,
) -> Result<Option<Box<dyn PDFSecurityHandler + Send + Sync>>> {
    if encrypt.is_empty() {
        return Ok(None);
    }

    // Get V and R values to determine handler type
    let v = get_int_default(encrypt, "V", 0);
    let r = get_int(encrypt, "R")?;

    match (v, r) {
        // V=1, R=2: 40-bit RC4
        (1, 2) => {
            let handler = PDFStandardSecurityHandlerV2::new(encrypt, doc_id, password)?;
            Ok(Some(Box::new(handler)))
        }
        // V=2, R=3: Variable-length RC4 (up to 128-bit)
        (2, 3) => {
            let handler = PDFStandardSecurityHandlerV2::new(encrypt, doc_id, password)?;
            Ok(Some(Box::new(handler)))
        }
        // V=4, R=4: AES-128
        (4, 4) => {
            let handler = PDFStandardSecurityHandlerV4::new(encrypt, doc_id, password)?;
            Ok(Some(Box::new(handler)))
        }
        // V=5, R=5: AES-256
        (5, 5) => {
            let handler = PDFStandardSecurityHandlerV5::new(encrypt, doc_id, password)?;
            Ok(Some(Box::new(handler)))
        }
        // V=5, R=6: AES-256 with complex password hash
        (5, 6) => {
            let handler = PDFStandardSecurityHandlerV5::new(encrypt, doc_id, password)?;
            Ok(Some(Box::new(handler)))
        }
        _ => Err(PdfError::EncryptionError(format!(
            "Unsupported encryption: V={}, R={}",
            v, r
        ))),
    }
}
