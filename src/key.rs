// Bitcoin secp256k1 bindings
// Written in 2014 by
//   Dawid Ciężarkiewicz
//   Andrew Poelstra
//
// To the extent possible under law, the author(s) have dedicated all
// copyright and related and neighboring rights to this software to
// the public domain worldwide. This software is distributed without
// any warranty.
//
// You should have received a copy of the CC0 Public Domain Dedication
// along with this software.
// If not, see <http://creativecommons.org/publicdomain/zero/1.0/>.
//

//! Public/Private keys

use std::intrinsics::copy_nonoverlapping;
use std::{fmt, marker, ops};
use rand::Rng;
use serialize::{Decoder, Decodable, Encoder, Encodable};
use serde::{Serialize, Deserialize, Serializer, Deserializer};

use super::Secp256k1;
use super::Error::{self, InvalidPublicKey, InvalidSecretKey, Unknown};
use constants;
use ffi;

/// Secret 256-bit key used as `x` in an ECDSA signature
pub struct SecretKey([u8; constants::SECRET_KEY_SIZE]);
impl_array_newtype!(SecretKey, u8, constants::SECRET_KEY_SIZE);

/// The number 1 encoded as a secret key
pub static ONE: SecretKey = SecretKey([0, 0, 0, 0, 0, 0, 0, 0,
                                       0, 0, 0, 0, 0, 0, 0, 0,
                                       0, 0, 0, 0, 0, 0, 0, 0,
                                       0, 0, 0, 0, 0, 0, 0, 1]);

/// Public key
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct PublicKey(PublicKeyData);

#[derive(Copy, Eq)]
enum PublicKeyData {
    Compressed([u8; constants::COMPRESSED_PUBLIC_KEY_SIZE]),
    Uncompressed([u8; constants::UNCOMPRESSED_PUBLIC_KEY_SIZE])
}

fn random_32_bytes<R: Rng>(rng: &mut R) -> [u8; 32] {
    let mut ret = [0u8; 32];
    rng.fill_bytes(&mut ret);
    ret
}

impl SecretKey {
    /// Creates a new random secret key
    #[inline]
    pub fn new<R: Rng>(secp: &Secp256k1, rng: &mut R) -> SecretKey {
        let mut data = random_32_bytes(rng);
        unsafe {
            while ffi::secp256k1_ec_seckey_verify(secp.ctx, data.as_ptr()) == 0 {
                data = random_32_bytes(rng);
            }
        }
        SecretKey(data)
    }

    /// Converts a `SECRET_KEY_SIZE`-byte slice to a secret key
    #[inline]
    pub fn from_slice(secp: &Secp256k1, data: &[u8])
                        -> Result<SecretKey, Error> {
        match data.len() {
            constants::SECRET_KEY_SIZE => {
                let mut ret = [0; constants::SECRET_KEY_SIZE];
                unsafe {
                    if ffi::secp256k1_ec_seckey_verify(secp.ctx, data.as_ptr()) == 0 {
                        return Err(InvalidSecretKey);
                    }
                    copy_nonoverlapping(data.as_ptr(),
                                        ret.as_mut_ptr(),
                                        data.len());
                }
                Ok(SecretKey(ret))
            }
            _ => Err(InvalidSecretKey)
        }
    }

    #[inline]
    /// Adds one secret key to another, modulo the curve order
    pub fn add_assign(&mut self, secp: &Secp256k1, other: &SecretKey)
                     -> Result<(), Error> {
        unsafe {
            if ffi::secp256k1_ec_privkey_tweak_add(secp.ctx, self.as_mut_ptr(), other.as_ptr()) != 1 {
                Err(Unknown)
            } else {
                Ok(())
            }
        }
    }
}

impl PublicKey {
    /// Creates a new zeroed out public key
    #[inline]
    pub fn new(compressed: bool) -> PublicKey {
        PublicKey(
            if compressed {
                PublicKeyData::Compressed([0; constants::COMPRESSED_PUBLIC_KEY_SIZE])
            } else {
                PublicKeyData::Uncompressed([0; constants::UNCOMPRESSED_PUBLIC_KEY_SIZE])
            }
        )
    }

    /// Creates a new public key from a secret key.
    #[inline]
    pub fn from_secret_key(secp: &Secp256k1,
                              sk: &SecretKey,
                              compressed: bool)
                             -> PublicKey {
        let mut pk = PublicKey::new(compressed);
        let compressed = if compressed {1} else {0};
        let mut len = 0;

        unsafe {
            // We can assume the return value because it's not possible to construct
            // an invalid `SecretKey` without transmute trickery or something
            let res = ffi::secp256k1_ec_pubkey_create(secp.ctx,
                                                      pk.as_mut_ptr(), &mut len,
                                                      sk.as_ptr(), compressed);
            debug_assert_eq!(res, 1);
        }
        debug_assert_eq!(len as usize, pk.len()); 
        pk
    }

    /// Creates a public key directly from a slice
    #[inline]
    pub fn from_slice(secp: &Secp256k1, data: &[u8])
                        -> Result<PublicKey, Error> {
        match data.len() {
            constants::COMPRESSED_PUBLIC_KEY_SIZE => {
                let mut ret = [0; constants::COMPRESSED_PUBLIC_KEY_SIZE];
                unsafe {
                    if ffi::secp256k1_ec_pubkey_verify(secp.ctx, data.as_ptr(),
                                                       data.len() as ::libc::c_int) == 0 {
                        return Err(InvalidPublicKey);
                    }
                    copy_nonoverlapping(data.as_ptr(),
                                        ret.as_mut_ptr(),
                                        data.len());
                }
                Ok(PublicKey(PublicKeyData::Compressed(ret)))
            }
            constants::UNCOMPRESSED_PUBLIC_KEY_SIZE => {
                let mut ret = [0; constants::UNCOMPRESSED_PUBLIC_KEY_SIZE];
                unsafe {
                    if ffi::secp256k1_ec_pubkey_verify(secp.ctx, data.as_ptr(),
                                                       data.len() as ::libc::c_int) == 0 {
                        return Err(InvalidPublicKey);
                    }
                    copy_nonoverlapping(data.as_ptr(),
                                        ret.as_mut_ptr(),
                                        data.len());
                }
                Ok(PublicKey(PublicKeyData::Uncompressed(ret)))
            }
            _ => Err(InvalidPublicKey)
        }
    }

    /// Returns whether the public key is compressed or uncompressed
    #[inline]
    pub fn is_compressed(&self) -> bool {
        let &PublicKey(ref data) = self;
        match *data {
            PublicKeyData::Compressed(_) => true,
            PublicKeyData::Uncompressed(_) => false
        }
    }

    /// Returns the length of the public key
    #[inline]
    pub fn len(&self) -> usize {
        let &PublicKey(ref data) = self;
        match *data {
            PublicKeyData::Compressed(ref x) => x.len(),
            PublicKeyData::Uncompressed(ref x) => x.len()
        }
    }

    /// Converts the public key to a raw pointer suitable for use
    /// with the FFI functions
    #[inline]
    pub fn as_ptr(&self) -> *const u8 {
        let &PublicKey(ref data) = self;
        match *data {
            PublicKeyData::Compressed(ref x) => x.as_ptr(),
            PublicKeyData::Uncompressed(ref x) => x.as_ptr()
        }
    }

    /// Converts the public key to a mutable raw pointer suitable for use
    /// with the FFI functions
    #[inline]
    pub fn as_mut_ptr(&mut self) -> *mut u8 {
        let &mut PublicKey(ref mut data) = self;
        match *data {
            PublicKeyData::Compressed(ref mut x) => x.as_mut_ptr(),
            PublicKeyData::Uncompressed(ref mut x) => x.as_mut_ptr()
        }
    }

    #[inline]
    /// Adds the pk corresponding to `other` to the pk `self` in place
    pub fn add_exp_assign(&mut self, secp: &Secp256k1, other: &SecretKey)
                         -> Result<(), Error> {
        unsafe {
            if ffi::secp256k1_ec_pubkey_tweak_add(secp.ctx, self.as_mut_ptr(),
                                                  self.len() as ::libc::c_int,
                                                  other.as_ptr()) != 1 {
                Err(Unknown)
            } else {
                Ok(())
            }
        }
    }
}

// We have to do all these impls ourselves as Rust can't derive
// them for arrays
impl Clone for PublicKeyData {
    fn clone(&self) -> PublicKeyData { *self }
}

impl PartialEq for PublicKeyData {
    fn eq(&self, other: &PublicKeyData) -> bool {
        &self[..] == &other[..]
    }
}

impl fmt::Debug for PublicKeyData {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for i in self[..].iter().cloned() {
            try!(write!(f, "{:02x}", i));
        }
        Ok(())
    }
}


impl ops::Index<usize> for PublicKeyData {
    type Output = u8;

    #[inline]
    fn index(&self, index: usize) -> &u8 {
        match *self {
            PublicKeyData::Compressed(ref x) => &x[index],
            PublicKeyData::Uncompressed(ref x) => &x[index]
       }
    }
}

impl ops::Index<usize> for PublicKey {
    type Output = u8;

    #[inline]
    fn index(&self, index: usize) -> &u8 {
        let &PublicKey(ref dat) = self;
        &dat[index]
    }
}

impl ops::Index<ops::Range<usize>> for PublicKeyData {
    type Output = [u8];

    #[inline]
    fn index(&self, index: ops::Range<usize>) -> &[u8] {
        match *self {
            PublicKeyData::Compressed(ref x) => &x[index],
            PublicKeyData::Uncompressed(ref x) => &x[index]
       }
    }
}

impl ops::Index<ops::Range<usize>> for PublicKey {
    type Output = [u8];

    #[inline]
    fn index(&self, index: ops::Range<usize>) -> &[u8] {
        let &PublicKey(ref dat) = self;
        &dat[index]
    }
}

impl ops::Index<ops::RangeTo<usize>> for PublicKeyData {
    type Output = [u8];

    #[inline]
    fn index(&self, index: ops::RangeTo<usize>) -> &[u8] {
        match *self {
            PublicKeyData::Compressed(ref x) => &x[index],
            PublicKeyData::Uncompressed(ref x) => &x[index]
       }
    }
}

impl ops::Index<ops::RangeTo<usize>> for PublicKey {
    type Output = [u8];

    #[inline]
    fn index(&self, index: ops::RangeTo<usize>) -> &[u8] {
        let &PublicKey(ref dat) = self;
        &dat[index]
    }
}

impl ops::Index<ops::RangeFrom<usize>> for PublicKeyData {
    type Output = [u8];

    #[inline]
    fn index(&self, index: ops::RangeFrom<usize>) -> &[u8] {
        match *self {
            PublicKeyData::Compressed(ref x) => &x[index],
            PublicKeyData::Uncompressed(ref x) => &x[index]
       }
    }
}

impl ops::Index<ops::RangeFrom<usize>> for PublicKey {
    type Output = [u8];

    #[inline]
    fn index(&self, index: ops::RangeFrom<usize>) -> &[u8] {
        let &PublicKey(ref dat) = self;
        &dat[index]
    }
}

impl ops::Index<ops::RangeFull> for PublicKeyData {
    type Output = [u8];

    #[inline]
    fn index(&self, _: ops::RangeFull) -> &[u8] {
        match *self {
            PublicKeyData::Compressed(ref x) => &x[..],
            PublicKeyData::Uncompressed(ref x) => &x[..]
       }
    }
}

impl ops::Index<ops::RangeFull> for PublicKey {
    type Output = [u8];

    #[inline]
    fn index(&self, _: ops::RangeFull) -> &[u8] {
        let &PublicKey(ref dat) = self;
        &dat[..]
    }
}

impl Decodable for PublicKey {
    fn decode<D: Decoder>(d: &mut D) -> Result<PublicKey, D::Error> {
        d.read_seq(|d, len| {
            if len == constants::UNCOMPRESSED_PUBLIC_KEY_SIZE {
                unsafe {
                    use std::mem;
                    let mut ret: [u8; constants::UNCOMPRESSED_PUBLIC_KEY_SIZE] = mem::uninitialized();
                    for i in 0..len {
                        ret[i] = try!(d.read_seq_elt(i, |d| Decodable::decode(d)));
                    }
                    Ok(PublicKey(PublicKeyData::Uncompressed(ret)))
                }
            } else if len == constants::COMPRESSED_PUBLIC_KEY_SIZE {
                unsafe {
                    use std::mem;
                    let mut ret: [u8; constants::COMPRESSED_PUBLIC_KEY_SIZE] = mem::uninitialized();
                    for i in 0..len {
                        ret[i] = try!(d.read_seq_elt(i, |d| Decodable::decode(d)));
                    }
                    Ok(PublicKey(PublicKeyData::Compressed(ret)))
                }
            } else {
                Err(d.error("Invalid length"))
            }
        })
    }
}

impl Encodable for PublicKey {
    fn encode<S: Encoder>(&self, s: &mut S) -> Result<(), S::Error> {
        (&self[..]).encode(s)
    }
}

impl Deserialize for PublicKey {
    fn deserialize<D>(d: &mut D) -> Result<PublicKey, D::Error>
        where D: Deserializer
    {
        use serde::de;
        struct Visitor {
            marker: marker::PhantomData<PublicKey>,
        }
        impl de::Visitor for Visitor {
            type Value = PublicKey;

            #[inline]
            fn visit_seq<V>(&mut self, mut v: V) -> Result<PublicKey, V::Error>
                where V: de::SeqVisitor
            {
                debug_assert!(constants::UNCOMPRESSED_PUBLIC_KEY_SIZE >= constants::COMPRESSED_PUBLIC_KEY_SIZE);

                unsafe {
                    use std::mem;
                    let mut ret_u: [u8; constants::UNCOMPRESSED_PUBLIC_KEY_SIZE] = mem::uninitialized();
                    let mut ret_c: [u8; constants::COMPRESSED_PUBLIC_KEY_SIZE] = mem::uninitialized();

                    let mut read_len = 0;
                    while read_len < constants::UNCOMPRESSED_PUBLIC_KEY_SIZE {
                        let read_ch = match try!(v.visit()) {
                            Some(c) => c,
                            None => break
                        };
                        ret_u[read_len] = read_ch;
                        if read_len < constants::COMPRESSED_PUBLIC_KEY_SIZE { ret_c[read_len] = read_ch; }
                        read_len += 1;
                    }
                    try!(v.end());

                    if read_len == constants::UNCOMPRESSED_PUBLIC_KEY_SIZE {
                        Ok(PublicKey(PublicKeyData::Uncompressed(ret_u)))
                    } else if read_len == constants::COMPRESSED_PUBLIC_KEY_SIZE {
                        Ok(PublicKey(PublicKeyData::Compressed(ret_c)))
                    } else {
                        return Err(de::Error::syntax_error());
                    }
                }
            }
        }

        // Begin actual function
        d.visit(Visitor { marker: ::std::marker::PhantomData })
    }
}

impl Serialize for PublicKey {
    fn serialize<S>(&self, s: &mut S) -> Result<(), S::Error>
        where S: Serializer
    {
        (&self.0[..]).serialize(s)
    }
}

impl fmt::Debug for SecretKey {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        try!(write!(f, "SecretKey("));
        for i in self[..].iter().cloned() {
            try!(write!(f, "{:02x}", i));
        }
        write!(f, ")")
    }
}

#[cfg(test)]
mod test {
    use super::super::Secp256k1;
    use super::super::Error::{InvalidPublicKey, InvalidSecretKey};
    use super::{PublicKey, SecretKey};
    use super::super::constants;

    use rand::{Rng, thread_rng};

    #[test]
    fn skey_from_slice() {
        let s = Secp256k1::new();
        let sk = SecretKey::from_slice(&s, &[1; 31]);
        assert_eq!(sk, Err(InvalidSecretKey));

        let sk = SecretKey::from_slice(&s, &[1; 32]);
        assert!(sk.is_ok());
    }

    #[test]
    fn pubkey_from_slice() {
        let s = Secp256k1::new();
        assert_eq!(PublicKey::from_slice(&s, &[]), Err(InvalidPublicKey));
        assert_eq!(PublicKey::from_slice(&s, &[1, 2, 3]), Err(InvalidPublicKey));

        let uncompressed = PublicKey::from_slice(&s, &[4, 54, 57, 149, 239, 162, 148, 175, 246, 254, 239, 75, 154, 152, 10, 82, 234, 224, 85, 220, 40, 100, 57, 121, 30, 162, 94, 156, 135, 67, 74, 49, 179, 57, 236, 53, 162, 124, 149, 144, 168, 77, 74, 30, 72, 211, 229, 110, 111, 55, 96, 193, 86, 227, 183, 152, 195, 155, 51, 247, 123, 113, 60, 228, 188]);
        assert!(uncompressed.is_ok());
        assert!(!uncompressed.unwrap().is_compressed());

        let compressed = PublicKey::from_slice(&s, &[3, 23, 183, 225, 206, 31, 159, 148, 195, 42, 67, 115, 146, 41, 248, 140, 11, 3, 51, 41, 111, 180, 110, 143, 114, 134, 88, 73, 198, 174, 52, 184, 78]);
        assert!(compressed.is_ok());
        assert!(compressed.unwrap().is_compressed());
    }

    #[test]
    fn keypair_slice_round_trip() {
        let s = Secp256k1::new();

        let (sk1, pk1) = s.generate_keypair(&mut thread_rng(), true).unwrap();
        assert_eq!(SecretKey::from_slice(&s, &sk1[..]), Ok(sk1));
        assert_eq!(PublicKey::from_slice(&s, &pk1[..]), Ok(pk1));

        let (sk2, pk2) = s.generate_keypair(&mut thread_rng(), false).unwrap();
        assert_eq!(SecretKey::from_slice(&s, &sk2[..]), Ok(sk2));
        assert_eq!(PublicKey::from_slice(&s, &pk2[..]), Ok(pk2));
    }

    #[test]
    fn invalid_secret_key() {
        let s = Secp256k1::new();
        // Zero
        assert_eq!(SecretKey::from_slice(&s, &[0; 32]), Err(InvalidSecretKey));
        // -1
        assert_eq!(SecretKey::from_slice(&s, &[0xff; 32]), Err(InvalidSecretKey));
        // Top of range
        assert!(SecretKey::from_slice(&s,
                                      &[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
                                        0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFE,
                                        0xBA, 0xAE, 0xDC, 0xE6, 0xAF, 0x48, 0xA0, 0x3B,
                                        0xBF, 0xD2, 0x5E, 0x8C, 0xD0, 0x36, 0x41, 0x40]).is_ok());
        // One past top of range
        assert!(SecretKey::from_slice(&s,
                                      &[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
                                        0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFE,
                                        0xBA, 0xAE, 0xDC, 0xE6, 0xAF, 0x48, 0xA0, 0x3B,
                                        0xBF, 0xD2, 0x5E, 0x8C, 0xD0, 0x36, 0x41, 0x41]).is_err());
    }

    #[test]
    fn test_bad_deserialize() {
        use std::io::Cursor;
        use serialize::{json, Decodable};

        let zero31 = "[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]".as_bytes();
        let json31 = json::Json::from_reader(&mut Cursor::new(zero31)).unwrap();
        let zero32 = "[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]".as_bytes();
        let json32 = json::Json::from_reader(&mut Cursor::new(zero32)).unwrap();
        let zero65 = "[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]".as_bytes();
        let json65 = json::Json::from_reader(&mut Cursor::new(zero65)).unwrap();
        let string = "\"my key\"".as_bytes();
        let json = json::Json::from_reader(&mut Cursor::new(string)).unwrap();

        // Invalid length
        let mut decoder = json::Decoder::new(json31.clone());
        assert!(<PublicKey as Decodable>::decode(&mut decoder).is_err());
        let mut decoder = json::Decoder::new(json31.clone());
        assert!(<SecretKey as Decodable>::decode(&mut decoder).is_err());
        let mut decoder = json::Decoder::new(json32.clone());
        assert!(<PublicKey as Decodable>::decode(&mut decoder).is_err());
        let mut decoder = json::Decoder::new(json32.clone());
        assert!(<SecretKey as Decodable>::decode(&mut decoder).is_ok());
        let mut decoder = json::Decoder::new(json65.clone());
        assert!(<PublicKey as Decodable>::decode(&mut decoder).is_ok());
        let mut decoder = json::Decoder::new(json65.clone());
        assert!(<SecretKey as Decodable>::decode(&mut decoder).is_err());

        // Syntax error
        let mut decoder = json::Decoder::new(json.clone());
        assert!(<PublicKey as Decodable>::decode(&mut decoder).is_err());
        let mut decoder = json::Decoder::new(json.clone());
        assert!(<SecretKey as Decodable>::decode(&mut decoder).is_err());
    }

    #[test]
    fn test_serialize() {
        use std::io::Cursor;
        use serialize::{json, Decodable, Encodable};

        macro_rules! round_trip (
            ($var:ident) => ({
                let start = $var;
                let mut encoded = String::new();
                {
                    let mut encoder = json::Encoder::new(&mut encoded);
                    start.encode(&mut encoder).unwrap();
                }
                let json = json::Json::from_reader(&mut Cursor::new(encoded.as_bytes())).unwrap();
                let mut decoder = json::Decoder::new(json);
                let decoded = Decodable::decode(&mut decoder);
                assert_eq!(Some(start), decoded.ok());
            })
        );

        let s = Secp256k1::new();
        for _ in 0..500 {
            let (sk, pk) = s.generate_keypair(&mut thread_rng(), false).unwrap();
            round_trip!(sk);
            round_trip!(pk);
            let (sk, pk) = s.generate_keypair(&mut thread_rng(), true).unwrap();
            round_trip!(sk);
            round_trip!(pk);
        }
    }

    #[test]
    fn test_bad_serde_deserialize() {
        use serde::{json, Deserialize};

        // Invalid length
        let zero31 = "[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]".as_bytes();
        let mut json = json::de::Deserializer::new(zero31.iter().map(|c| Ok(*c))).unwrap();
        assert!(<PublicKey as Deserialize>::deserialize(&mut json).is_err());
        let mut json = json::de::Deserializer::new(zero31.iter().map(|c| Ok(*c))).unwrap();
        assert!(<SecretKey as Deserialize>::deserialize(&mut json).is_err());

        let zero32 = "[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]".as_bytes();
        let mut json = json::de::Deserializer::new(zero32.iter().map(|c| Ok(*c))).unwrap();
        assert!(<PublicKey as Deserialize>::deserialize(&mut json).is_err());
        let mut json = json::de::Deserializer::new(zero32.iter().map(|c| Ok(*c))).unwrap();
        assert!(<SecretKey as Deserialize>::deserialize(&mut json).is_ok());

        let zero65 = "[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]".as_bytes();
        let mut json = json::de::Deserializer::new(zero65.iter().map(|c| Ok(*c))).unwrap();
        assert!(<PublicKey as Deserialize>::deserialize(&mut json).is_ok());
        let mut json = json::de::Deserializer::new(zero65.iter().map(|c| Ok(*c))).unwrap();
        assert!(<SecretKey as Deserialize>::deserialize(&mut json).is_err());

        // Syntax error
        let string = "\"my key\"".as_bytes();
        let mut json = json::de::Deserializer::new(string.iter().map(|c| Ok(*c))).unwrap();
        assert!(<PublicKey as Deserialize>::deserialize(&mut json).is_err());
        let mut json = json::de::Deserializer::new(string.iter().map(|c| Ok(*c))).unwrap();
        assert!(<SecretKey as Deserialize>::deserialize(&mut json).is_err());
    }


    #[test]
    fn test_serialize_serde() {
        use serde::{json, Serialize, Deserialize};

        macro_rules! round_trip (
            ($var:ident) => ({
                let start = $var;
                let mut encoded = Vec::new();
                {
                    let mut serializer = json::ser::Serializer::new(&mut encoded);
                    start.serialize(&mut serializer).unwrap();
                }
                let mut deserializer = json::de::Deserializer::new(encoded.iter().map(|c| Ok(*c))).unwrap();
                let decoded = Deserialize::deserialize(&mut deserializer);
                assert_eq!(Some(start), decoded.ok());
            })
        );

        let s = Secp256k1::new();
        for _ in 0..500 {
            let (sk, pk) = s.generate_keypair(&mut thread_rng(), false).unwrap();
            round_trip!(sk);
            round_trip!(pk);
            let (sk, pk) = s.generate_keypair(&mut thread_rng(), true).unwrap();
            round_trip!(sk);
            round_trip!(pk);
        }
    }

    #[test]
    fn test_out_of_range() {

        struct BadRng(u8);
        impl Rng for BadRng {
            fn next_u32(&mut self) -> u32 { unimplemented!() }
            // This will set a secret key to a little over the
            // group order, then decrement with repeated calls
            // until it returns a valid key
            fn fill_bytes(&mut self, data: &mut [u8]) {
                let group_order: [u8; 32] = [
                    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
                    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xfe,
                    0xba, 0xae, 0xdc, 0xe6, 0xaf, 0x48, 0xa0, 0x3b,
                    0xbf, 0xd2, 0x5e, 0x8c, 0xd0, 0x36, 0x41, 0x41];
                assert_eq!(data.len(), 32);
                unsafe {
                    use std::intrinsics::copy_nonoverlapping;
                    copy_nonoverlapping(group_order.as_ptr(),
                                        data.as_mut_ptr(),
                                        32);
                }
                data[31] = self.0;
                self.0 -= 1;
            }
        }

        let s = Secp256k1::new();
        s.generate_keypair(&mut BadRng(0xff), false).unwrap();
        s.generate_keypair(&mut BadRng(0xff), true).unwrap();
    }

    #[test]
    fn test_pubkey_from_bad_slice() {
        let s = Secp256k1::new();
        // Bad sizes
        assert_eq!(PublicKey::from_slice(&s, &[0; constants::COMPRESSED_PUBLIC_KEY_SIZE - 1]),
                   Err(InvalidPublicKey));
        assert_eq!(PublicKey::from_slice(&s, &[0; constants::COMPRESSED_PUBLIC_KEY_SIZE + 1]),
                   Err(InvalidPublicKey));
        assert_eq!(PublicKey::from_slice(&s, &[0; constants::UNCOMPRESSED_PUBLIC_KEY_SIZE - 1]),
                   Err(InvalidPublicKey));
        assert_eq!(PublicKey::from_slice(&s, &[0; constants::UNCOMPRESSED_PUBLIC_KEY_SIZE + 1]),
                   Err(InvalidPublicKey));

        // Bad parse
        assert_eq!(PublicKey::from_slice(&s, &[0xff; constants::UNCOMPRESSED_PUBLIC_KEY_SIZE]),
                   Err(InvalidPublicKey));
        assert_eq!(PublicKey::from_slice(&s, &[0x55; constants::COMPRESSED_PUBLIC_KEY_SIZE]),
                   Err(InvalidPublicKey));
    }

    #[test]
    fn test_debug_output() {
        struct DumbRng(u32);
        impl Rng for DumbRng {
            fn next_u32(&mut self) -> u32 {
                self.0 = self.0.wrapping_add(1);
                self.0
            }
        }

        let s = Secp256k1::new();
        let (sk1, pk1) = s.generate_keypair(&mut DumbRng(0), false).unwrap();
        let (sk2, pk2) = s.generate_keypair(&mut DumbRng(0), true).unwrap();

        assert_eq!(&format!("{:?}", sk1),
                   "SecretKey(0200000001000000040000000300000006000000050000000800000007000000)");
        assert_eq!(&format!("{:?}", pk1),
                   "PublicKey(049510c48c265cefb3413be0e6b75beef02ebafcaf6634f962b27b4832abc4feec01bd8ff2e31057f7b7a244ed8c5ccd9781a63a6f607b40b493330cd159ecd5ce)");
        assert_eq!(&format!("{:?}", sk2),
                   "SecretKey(0200000001000000040000000300000006000000050000000800000007000000)");
        assert_eq!(&format!("{:?}", pk2),
                   "PublicKey(029510c48c265cefb3413be0e6b75beef02ebafcaf6634f962b27b4832abc4feec)");
    }

    #[test]
    fn test_addition() {
        let s = Secp256k1::new();

        let (mut sk1, mut pk1) = s.generate_keypair(&mut thread_rng(), true).unwrap();
        let (mut sk2, mut pk2) = s.generate_keypair(&mut thread_rng(), true).unwrap();

        assert_eq!(PublicKey::from_secret_key(&s, &sk1, true), pk1);
        assert!(sk1.add_assign(&s, &sk2).is_ok());
        assert!(pk1.add_exp_assign(&s, &sk2).is_ok());
        assert_eq!(PublicKey::from_secret_key(&s, &sk1, true), pk1);

        assert_eq!(PublicKey::from_secret_key(&s, &sk2, true), pk2);
        assert!(sk2.add_assign(&s, &sk1).is_ok());
        assert!(pk2.add_exp_assign(&s, &sk1).is_ok());
        assert_eq!(PublicKey::from_secret_key(&s, &sk2, true), pk2);
    }
}


