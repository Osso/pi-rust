use sha2::{Digest, Sha256};

const VERIFIER_BYTES: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pkce {
    pub verifier: String,
    pub challenge: String,
}

pub fn generate_pkce() -> Pkce {
    generate_pkce_with(|bytes| {
        getrandom::fill(bytes).expect("OS random source must be available for PKCE");
    })
}

/// PKCE generation with an injected random byte source.
pub fn generate_pkce_with(mut fill: impl FnMut(&mut [u8])) -> Pkce {
    let mut verifier_bytes = [0_u8; VERIFIER_BYTES];
    fill(&mut verifier_bytes);
    let verifier = base64url_encode(&verifier_bytes);
    let challenge = challenge_for(&verifier);

    Pkce { verifier, challenge }
}

pub fn challenge_for(verifier: &str) -> String {
    let digest = Sha256::digest(verifier.as_bytes());
    base64url_encode(&digest)
}

fn base64url_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

    let mut encoded = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let first = chunk[0];
        let second = chunk.get(1).copied().unwrap_or(0);
        let third = chunk.get(2).copied().unwrap_or(0);

        encoded.push(ALPHABET[(first >> 2) as usize] as char);
        encoded.push(ALPHABET[(((first & 0b11) << 4) | (second >> 4)) as usize] as char);
        if chunk.len() > 1 {
            encoded.push(ALPHABET[(((second & 0b1111) << 2) | (third >> 6)) as usize] as char);
        }
        if chunk.len() > 2 {
            encoded.push(ALPHABET[(third & 0b111111) as usize] as char);
        }
    }
    encoded
}
