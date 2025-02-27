use base64::{engine::general_purpose, Engine};
use candid::Principal;
use ciborium::{from_reader, into_writer};
use hmac::{Hmac, Mac};
use serde::{de::DeserializeOwned, Serialize};
use serde_bytes::ByteBuf;
use sha3::{Digest, Sha3_256};

pub use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};

pub fn sha256(data: &[u8]) -> [u8; 32] {
    let mut hasher = sha2::Sha256::new();
    hasher.update(data);
    hasher.finalize().into()
}

pub fn sha3_256(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha3_256::new();
    hasher.update(data);
    hasher.finalize().into()
}

pub fn mac_256(key: &[u8], add: &[u8]) -> [u8; 32] {
    let mut mac = Hmac::<Sha3_256>::new_from_slice(key).expect("HMAC can take key of any size");
    mac.update(add);
    mac.finalize().into_bytes().into()
}

pub fn mac_256_2(key: &[u8], add1: &[u8], add2: &[u8]) -> [u8; 32] {
    let mut mac = Hmac::<Sha3_256>::new_from_slice(key).expect("HMAC can take key of any size");
    mac.update(add1);
    mac.update(add2);
    mac.finalize().into_bytes().into()
}

// to_cbor_bytes returns the CBOR encoding of the given object that implements the Serialize trait.
pub fn to_cbor_bytes(obj: &impl Serialize) -> Vec<u8> {
    let mut buf: Vec<u8> = Vec::new();
    into_writer(obj, &mut buf).expect("failed to encode in CBOR format");
    buf
}

pub fn bytes32_from_base64(s: &str) -> Result<[u8; 32], String> {
    let bytes = general_purpose::URL_SAFE_NO_PAD
        .decode(s)
        .map_err(|_| "failed to decode base64")?;
    if bytes.len() != 32 {
        return Err(format!("expected 32 bytes, got {}", bytes.len()));
    }
    let mut result = [0u8; 32];
    result.copy_from_slice(&bytes);
    Ok(result)
}

// Challenge is a trait for generating and verifying challenges.
pub trait Challenge {
    fn challenge(&self, key: &[u8], timestamp: u64) -> Vec<u8>;
    fn verify(&self, key: &[u8], expire_at: u64, challenge: &[u8]) -> Result<(), String>;
}

// Implement the Challenge trait for any type that implements the Serialize trait.
impl<T> Challenge for T
where
    T: Serialize,
{
    fn challenge(&self, key: &[u8], timestamp: u64) -> Vec<u8> {
        let mac = &mac_256_2(key, &to_cbor_bytes(self), &to_cbor_bytes(&timestamp))[0..16];
        to_cbor_bytes(&(timestamp, ByteBuf::from(mac)))
    }

    fn verify(&self, key: &[u8], expire_at: u64, challenge: &[u8]) -> Result<(), String> {
        let arr: (u64, ByteBuf) =
            from_reader(challenge).map_err(|_err| "failed to decode the challenge")?;

        if arr.0 < expire_at {
            return Err("the challenge is expired".to_string());
        }

        let mac = &mac_256_2(key, &to_cbor_bytes(self), &to_cbor_bytes(&arr.0))[0..16];
        if mac != &arr.1[..] {
            return Err("failed to verify the challenge".to_string());
        }

        Ok(())
    }
}

pub trait Cryptogram {
    fn encode(&self, key: &[u8], subject: Option<Principal>) -> String;
    fn decode(key: &[u8], subject: Option<Principal>, cryptogram: &str) -> Result<Self, String>
    where
        Self: Sized;
}

impl<T> Cryptogram for T
where
    T: Serialize + DeserializeOwned,
{
    fn encode(&self, key: &[u8], subject: Option<Principal>) -> String {
        let data = to_cbor_bytes(self);
        let mac = match subject {
            Some(subject) => mac_256_2(key, &data, subject.as_slice()),
            None => mac_256(key, &data),
        };

        let data = to_cbor_bytes(&[ByteBuf::from(data), ByteBuf::from(&mac[0..8])]);
        general_purpose::URL_SAFE_NO_PAD.encode(data)
    }

    fn decode(key: &[u8], subject: Option<Principal>, cryptogram: &str) -> Result<Self, String> {
        let data = general_purpose::URL_SAFE_NO_PAD
            .decode(cryptogram)
            .map_err(|_err| "failed to decode base64 cryptogram")?;
        let arr: (ByteBuf, ByteBuf) =
            from_reader(&data[..]).map_err(|_err| "failed to decode cryptogram")?;
        let mac = match subject {
            Some(subject) => mac_256_2(key, &arr.0, subject.as_slice()),
            None => mac_256(key, &arr.0),
        };
        if &mac[0..8] != arr.1.as_slice() {
            return Err("failed to verify the cryptogram".to_string());
        }
        from_reader(arr.0.as_slice()).map_err(|_err| "failed to decode cryptogram".to_string())
    }
}

pub trait Ed25519Message {
    fn sign_to(&self, key: &SigningKey) -> String;
    fn verify_from(key: &VerifyingKey, msg: &str) -> Result<Self, String>
    where
        Self: Sized;
}

impl<T> Ed25519Message for T
where
    T: Serialize + DeserializeOwned,
{
    fn sign_to(&self, key: &SigningKey) -> String {
        let data = to_cbor_bytes(self);
        let sig = key.sign(&data).to_bytes();

        let data = to_cbor_bytes(&[ByteBuf::from(data), ByteBuf::from(sig)]);
        general_purpose::URL_SAFE_NO_PAD.encode(data)
    }

    fn verify_from(key: &VerifyingKey, msg: &str) -> Result<Self, String> {
        let data = general_purpose::URL_SAFE_NO_PAD
            .decode(msg)
            .map_err(|_err| "failed to decode base64 message")?;
        let arr: (ByteBuf, ByteBuf) =
            from_reader(&data[..]).map_err(|_err| "failed to decode Ed25519 message")?;
        let sig = Signature::from_slice(arr.1.as_slice())
            .map_err(|_err| "failed to parse Ed25519 signature")?;
        key.verify_strict(arr.0.as_slice(), &sig)
            .map_err(|_| "failed to verify Ed25519 signature")?;
        from_reader(arr.0.as_slice()).map_err(|_err| "failed to decode Ed25519 message".to_string())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use serde::Deserialize;

    #[test]
    fn test_challenge() {
        let key = b"secret key";
        let challenge = "challenge";
        let expire_at = 1000;
        let c = challenge.challenge(key, expire_at);
        println!("challenge: {}, {:?}", c.len(), c);
        assert!(c.len() < 24);
        assert!(challenge.verify(key, expire_at, &c).is_ok());
        assert!(challenge.verify(key, expire_at, &c[1..]).is_err());
        assert!(challenge.verify(&key[1..], expire_at, &c).is_err());
        assert!(challenge.verify(key, expire_at + 1, &c).is_err());
    }

    #[test]
    fn test_cryptogram() {
        // Prize format: (Issuer code, Issue time, Expire, Claimable tokens, Quantity)
        #[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
        struct Prize(pub u32, pub u32, pub u16, pub u32, pub u16);

        let key = b"secret key";
        let prize = Prize(0, 999, 0, 0, 0);
        let subject = Principal::anonymous();
        let cryptogram = prize.encode(key, Some(subject));
        println!("cryptogram: {}", cryptogram); // gkiFABkD5wAAAEjSOJCPQS-eDw
        let res = Prize::decode(key, Some(subject), &cryptogram).unwrap();
        assert_eq!(prize, res);
        assert!(Prize::decode(key, None, &cryptogram).is_err());

        let prize = Prize(u32::MAX, u32::MAX, u16::MAX, u32::MAX, u16::MAX);
        let cryptogram = prize.encode(key, None);
        println!("cryptogram: {}", cryptogram); // glaFGv____8a_____xn__xr_____Gf__SBt10SXZQ3eD
        let res = Prize::decode(key, None, &cryptogram).unwrap();
        assert_eq!(prize, res);
        assert!(Prize::decode(key, Some(subject), &cryptogram).is_err());
        assert!(Prize::decode(&key[1..], None, &cryptogram).is_err());
    }

    #[test]
    fn test_ed25519_message() {
        #[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
        pub struct ChallengeState(pub (Principal, String, u64));

        let sk = SigningKey::from_bytes(&sha3_256(b"secret key"));
        let pk = VerifyingKey::from(&sk);
        let state = ChallengeState((Principal::anonymous(), "1234567890".to_string(), 1000));
        let msg = state.sign_to(&sk);
        println!("message: {}", msg); // message: glGDQQRqMTIzNDU2Nzg5MBkD6FhAyABxVV7f4L9QXL_MP0-VZE5EMzu288JeF0kHz4FxvByZlHaSmZx_BCEIxOouOY0CCgJEuTxkUnpZF24EpQozBw
        let state2 = ChallengeState::verify_from(&pk, &msg).unwrap();
        assert_eq!(state, state2);
    }
}
