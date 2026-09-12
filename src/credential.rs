//! The per-invocation secret that scopes the proxy to sandme's own child
//! (SPEC-0003 FR-203).

use std::fmt::Write as _;
use std::hash::{BuildHasher, RandomState};
use std::io::Read;
use std::net::SocketAddr;

/// The user half of the credential; the password half is the per-run secret.
const USER: &str = "sandme";

/// Bytes of secret behind one credential (NFR-201 asks for 128 bits).
const SECRET_BYTES: usize = 16;

/// A credential the proxy requires and the sandboxed command presents.
///
/// It exists for one invocation, lives in memory, and reaches the child in
/// the proxy URL sandme already puts in its environment.
pub struct Credential {
    secret: String,
}

impl Credential {
    /// A credential for this invocation, from the OS entropy source (NFR-201).
    ///
    /// `/dev/urandom` is the source. If it cannot be read, the secret falls
    /// back to `RandomState`'s OS-seeded hash keys rather than to nothing:
    /// FR-203 has the proxy require a credential on every request, and a
    /// proxy that came up without one would be the open relay that
    /// requirement exists to close.
    pub fn build() -> Self {
        let mut bytes = [0_u8; SECRET_BYTES];

        if read_random(&mut bytes).is_none() {
            for (chunk, seed) in bytes.chunks_mut(8).zip(0_u8..) {
                chunk.copy_from_slice(&RandomState::new().hash_one(seed).to_ne_bytes());
            }
        }

        let secret = bytes.iter().fold(String::new(), |mut hex, byte| {
            let _ = write!(hex, "{byte:02x}");
            hex
        });
        Self { secret }
    }

    /// The proxy URL for the sandboxed command, credential included.
    ///
    /// The secret travels as URL userinfo because HTTP clients already turn
    /// that into a `Proxy-Authorization: Basic` header when they read
    /// `HTTP_PROXY`. The child therefore authenticates without learning
    /// anything new, and the relay is scoped to it.
    pub fn proxy_url(&self, proxy: SocketAddr) -> String {
        format!("http://{USER}:{}@{proxy}", self.secret)
    }

    /// The header value a client configured from [`Credential::proxy_url`]
    /// sends, which is what the proxy compares each request against.
    pub fn expected_authorization(&self) -> String {
        format!("Basic {}", base64(&format!("{USER}:{}", self.secret)))
    }
}

/// Fill `bytes` from the operating system's entropy source.
fn read_random(bytes: &mut [u8]) -> Option<()> {
    std::fs::File::open("/dev/urandom")
        .and_then(|mut source| source.read_exact(bytes))
        .ok()
}

/// Base64-encode `input` (RFC 4648 §4), for the `Basic` credential.
///
/// Twenty lines against a new dependency for one header value, per the
/// dependency rule in `AGENTS.md`. `pub(crate)` so the SSH tunnel
/// ([`crate::tunnel`]) encodes the same credential the same way when it reads
/// it back from `HTTP_PROXY`, rather than growing a second encoder.
pub(crate) fn base64(input: &str) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut encoded = String::new();
    for chunk in input.as_bytes().chunks(3) {
        let block = chunk
            .iter()
            .copied()
            .chain([0, 0])
            .take(3)
            .fold(0_u32, |block, byte| (block << 8) | u32::from(byte));

        for (index, shift) in [18, 12, 6, 0].into_iter().enumerate() {
            if index <= chunk.len() {
                encoded.push(char::from(
                    ALPHABET[(block >> shift) as usize & 0b0011_1111],
                ));
            } else {
                encoded.push('=');
            }
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[test]
    fn publishes_a_secret_the_child_can_present() {
        // Given two credentials, as two invocations would build them
        let first = Credential::build();
        let second = Credential::build();
        let proxy = SocketAddr::from((Ipv4Addr::LOCALHOST, 8787));

        // Then each publishes a proxy URL carrying 128 bits of hex secret
        let url = first.proxy_url(proxy);
        assert_eq!(
            url,
            format!("http://sandme:{}@127.0.0.1:8787", first.secret)
        );
        assert_eq!(first.secret.len(), SECRET_BYTES * 2);
        assert!(first.secret.chars().all(|c| c.is_ascii_hexdigit()));

        // And no two invocations share one (NFR-201)
        assert_ne!(first.secret, second.secret);
    }

    #[test]
    fn expects_what_a_client_reading_the_proxy_url_sends() {
        // Given a credential with a known secret
        let credential = Credential {
            secret: "0123456789abcdef".to_string(),
        };

        // Then the expected header is the Basic encoding of the URL userinfo,
        // which is what curl, reqwest and their peers put on the wire
        assert_eq!(
            credential.expected_authorization(),
            "Basic c2FuZG1lOjAxMjM0NTY3ODlhYmNkZWY="
        );
    }

    #[test]
    fn encodes_base64_at_every_padding_length() {
        // Given the RFC 4648 test vectors
        assert_eq!(base64(""), "");
        assert_eq!(base64("f"), "Zg==");
        assert_eq!(base64("fo"), "Zm8=");
        assert_eq!(base64("foo"), "Zm9v");
        assert_eq!(base64("foob"), "Zm9vYg==");
        assert_eq!(base64("fooba"), "Zm9vYmE=");
        assert_eq!(base64("foobar"), "Zm9vYmFy");
    }
}
