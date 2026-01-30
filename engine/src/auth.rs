use std::path::Path;

use alloy_rpc_types_engine::{Claims, JwtSecret};
use color_eyre::eyre::{self, Ok};
use jsonwebtoken::{encode, get_current_timestamp, Algorithm, EncodingKey, Header};

/// Default algorithm used for JWT token signing.
const DEFAULT_ALGORITHM: Algorithm = Algorithm::HS256;

/// Contains the JWT secret and claims parameters.
pub struct Auth {
    key: EncodingKey,
}

impl Auth {
    pub fn new(secret: JwtSecret) -> Self {
        Self {
            key: EncodingKey::from_secret(secret.as_bytes()),
        }
    }

    /// Create a new `Auth` struct given the path to the file containing the hex
    /// encoded jwt key.
    pub fn new_from_path(jwt_path: &Path) -> eyre::Result<Self> {
        Ok(Self::new(JwtSecret::from_file(jwt_path)?))
    }

    /// Generate a JWT token with `claims.iat` set to current time.
    pub fn generate_token(&self) -> eyre::Result<String> {
        let claims = self.generate_claims_at_timestamp();
        self.generate_token_with_claims(&claims)
    }

    /// Generate a JWT token with the given claims.
    fn generate_token_with_claims(&self, claims: &Claims) -> eyre::Result<String> {
        let header = Header::new(DEFAULT_ALGORITHM);
        Ok(encode(&header, claims, &self.key)?)
    }

    /// Generate a `Claims` struct with `iat` set to current time
    fn generate_claims_at_timestamp(&self) -> Claims {
        Claims {
            iat: get_current_timestamp(),
            exp: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_roundtrip() {
        let secret = JwtSecret::random();
        let auth = Auth::new(secret);
        let claims = auth.generate_claims_at_timestamp();
        let token = auth.generate_token_with_claims(&claims).unwrap();

        let res = secret.validate(token.as_str());
        assert!(res.is_ok());
    }
}
