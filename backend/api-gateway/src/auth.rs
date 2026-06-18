use std::{
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use axum::http::{header, HeaderMap};
use jsonwebtoken::{
    decode, decode_header,
    errors::{Error as JwtError, ErrorKind as JwtErrorKind},
    Algorithm, DecodingKey, Validation,
};
use serde::Deserialize;
use serde_json::Value;
use tokio::sync::RwLock;

use crate::{config::AuthConfig, error::GatewayError};

#[derive(Debug, Clone)]
pub struct AdminContext {
    pub subject: String,
    pub roles: Vec<String>,
}

#[derive(Clone)]
pub struct AuthService {
    config: AuthConfig,
    client: reqwest::Client,
    jwks: Arc<RwLock<Option<CachedJwks>>>,
}

impl AuthService {
    pub fn new(config: AuthConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
            jwks: Arc::new(RwLock::new(None)),
        }
    }

    #[cfg(test)]
    async fn authenticate(&self, headers: &HeaderMap) -> Result<AdminContext, GatewayError> {
        self.authenticate_with_role(headers, &self.config.admin_required_role)
            .await
    }

    pub async fn authenticate_with_role(
        &self,
        headers: &HeaderMap,
        required_role: &str,
    ) -> Result<AdminContext, GatewayError> {
        let token = bearer_token(headers)?;
        let header = decode_header(token)
            .map_err(|_| GatewayError::unauthenticated("invalid bearer token"))?;

        if header.alg != Algorithm::RS256 {
            return Err(GatewayError::unauthenticated("unsupported token algorithm"));
        }

        let kid = header
            .kid
            .ok_or_else(|| GatewayError::unauthenticated("token key id is missing"))?;
        let mut jwks = self.cached_jwks().await?;
        let mut decoding_key = jwks.decoding_key(&kid);

        if decoding_key.is_none() {
            jwks = self.refresh_jwks().await?;
            decoding_key = jwks.decoding_key(&kid);
        }

        let decoding_key = decoding_key
            .ok_or_else(|| GatewayError::unauthenticated("token signing key is unknown"))?;
        let validation = validation_for_config(&self.config);

        let token_data =
            decode::<Claims>(token, &decoding_key, &validation).map_err(jwt_decode_error)?;
        let claims = token_data.claims;

        validate_registered_claims(&self.config, &claims)?;
        let roles = roles_from_claim(&claims.extra, &self.config.admin_role_claim);

        if !roles.iter().any(|role| role == required_role) {
            return Err(GatewayError::forbidden("admin role is required"));
        }

        Ok(AdminContext {
            subject: claims.sub,
            roles,
        })
    }

    async fn cached_jwks(&self) -> Result<Jwks, GatewayError> {
        let cached = self.jwks.read().await.clone();
        if let Some(cached) = cached {
            if cached.fetched_at.elapsed() <= self.config.jwks_cache_ttl {
                return Ok(cached.keys);
            }
        }

        self.refresh_jwks().await
    }

    async fn refresh_jwks(&self) -> Result<Jwks, GatewayError> {
        let keys = self
            .client
            .get(&self.config.jwks_url)
            .send()
            .await
            .map_err(|_| GatewayError::service_unavailable("JWKS endpoint is unavailable"))?
            .error_for_status()
            .map_err(|_| GatewayError::service_unavailable("JWKS endpoint returned an error"))?
            .json::<Jwks>()
            .await
            .map_err(|_| GatewayError::service_unavailable("JWKS response is invalid"))?;

        *self.jwks.write().await = Some(CachedJwks {
            keys: keys.clone(),
            fetched_at: Instant::now(),
        });

        Ok(keys)
    }
}

fn validation_for_config(config: &AuthConfig) -> Validation {
    let mut validation = Validation::new(Algorithm::RS256);
    validation.leeway = config.jwt_clock_skew.as_secs();
    validation.validate_exp = true;
    validation.validate_nbf = true;
    validation.set_audience(&[config.audience.as_str()]);
    validation.set_issuer(&[config.issuer.as_str()]);

    validation
}

fn jwt_decode_error(error: JwtError) -> GatewayError {
    match error.kind() {
        JwtErrorKind::ExpiredSignature => GatewayError::unauthenticated("token is expired"),
        JwtErrorKind::ImmatureSignature => GatewayError::unauthenticated("token is not yet valid"),
        JwtErrorKind::InvalidAudience => GatewayError::unauthenticated("invalid token audience"),
        JwtErrorKind::InvalidIssuer => GatewayError::unauthenticated("invalid token issuer"),
        _ => GatewayError::unauthenticated("invalid bearer token"),
    }
}

fn bearer_token(headers: &HeaderMap) -> Result<&str, GatewayError> {
    let value = headers
        .get(header::AUTHORIZATION)
        .ok_or_else(|| GatewayError::unauthenticated("authentication required"))?
        .to_str()
        .map_err(|_| GatewayError::unauthenticated("invalid authorization header"))?;

    value
        .strip_prefix("Bearer ")
        .filter(|token| !token.trim().is_empty())
        .ok_or_else(|| GatewayError::unauthenticated("bearer token is required"))
}

fn validate_registered_claims(config: &AuthConfig, claims: &Claims) -> Result<(), GatewayError> {
    let now = unix_timestamp();
    let skew = config.jwt_clock_skew.as_secs();

    if let Some(expected_tenant_id) = config.tenant_id.as_deref() {
        if claims.tid.as_deref() != Some(expected_tenant_id) {
            return Err(GatewayError::unauthenticated("invalid token tenant"));
        }
    }

    if claims.sub.trim().is_empty() {
        return Err(GatewayError::unauthenticated("token subject is missing"));
    }

    if claims.iat > now.saturating_add(skew) {
        return Err(GatewayError::unauthenticated(
            "token issued-at time is invalid",
        ));
    }

    if claims.exp.saturating_sub(claims.iat) > config.max_admin_token_lifetime.as_secs() {
        return Err(GatewayError::unauthenticated("token lifetime is too long"));
    }

    Ok(())
}

fn roles_from_claim(extra: &serde_json::Map<String, Value>, claim_name: &str) -> Vec<String> {
    match extra.get(claim_name) {
        Some(Value::String(value)) => vec![value.clone()],
        Some(Value::Array(values)) => values
            .iter()
            .filter_map(Value::as_str)
            .map(ToOwned::to_owned)
            .collect(),
        _ => Vec::new(),
    }
}

fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_secs()
}

#[derive(Debug, Deserialize)]
struct Claims {
    sub: String,
    tid: Option<String>,
    exp: u64,
    iat: u64,
    #[serde(flatten)]
    extra: serde_json::Map<String, Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Jwks {
    keys: Vec<Jwk>,
}

impl Jwks {
    fn decoding_key(&self, kid: &str) -> Option<DecodingKey> {
        self.keys
            .iter()
            .find(|key| key.kid == kid && key.kty == "RSA")
            .and_then(|key| DecodingKey::from_rsa_components(&key.n, &key.e).ok())
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Jwk {
    kid: String,
    kty: String,
    n: String,
    e: String,
}

#[derive(Clone)]
struct CachedJwks {
    keys: Jwks,
    fetched_at: Instant,
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use axum::{
        http::{header, HeaderMap, HeaderValue, StatusCode},
        response::IntoResponse,
        routing::get,
        Router,
    };
    use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
    use serde::Serialize;
    use serde_json::{json, Value};
    use tokio::net::TcpListener;

    use crate::config::AuthConfig;

    use super::{roles_from_claim, validate_registered_claims, AuthService, Claims};

    const TEST_KID: &str = "lumenhorizon-test-key";
    const TEST_TENANT_ID: &str = "11111111-1111-1111-1111-111111111111";
    const TEST_PRIVATE_KEY_PEM: &str = r#"-----BEGIN PRIVATE KEY-----
MIIEvgIBADANBgkqhkiG9w0BAQEFAASCBKgwggSkAgEAAoIBAQCjA/+zpnWWlBCH
NQoYvAZt2h+lOQYfHad3viIWJxza5xvd3/zYw6MHx3cplH+aEtN6THkViRWWA0b1
9J0FoRPSQV13EcugeS4iJoF6GHBrP5YUBQ7PvMnVNLgR94Gc0ODa1E19/pPaRjKT
sAxcAeyRKloAscmvnzGjGYwPXAVVNejqNxME6a+RvXXF/RGtfQ+agdBREWiYm8S3
qf8bOboDDLnxd4RZVPrBk8vPIuj9Cth4wRoz15t+7hgNXCJSsZAoZ2Bz64IBN4HQ
OH91+luFHM4iDCluniqezECz5/AHs/BKt8ndoq4JbFreradtbjzjnxEJPlAU2ocD
39b9e6OHAgMBAAECggEATOcFCPY9QNUG2xINek+xZL9i8GnvVlyB9X5PzE2VUjt2
rPwO5x+b2H6j24y/iarZ8zcCZENwWH2hS0SjktVDaYwGkLZnboPcXKX3rOa8mgu8
PCOKsjuop5kgQPGXRqhPe0xuZkLj8zPshjmmCv2pYW4uWWeF8wDRxNS3e0N5DJIK
AocYf//185reEB6ArFJbyQnbAzXlKdPySohg+r4DsDRWDwvFkHb5G7qsk/UY398P
BR3/heO+UgJs5fziruLtM5oMUPyrub+4DhyMJzDb70J1y8OV6sEffc/S4NT5iDtd
v1Ady2zxbq0RI12ITXI+1K54/EIrvzMsnsVSFtjKyQKBgQDYxUayRob1AT0cYEi0
hkigWAoSxef8FCpteptmsiClY7DcOYslcQ/dGfdYIW9oq7NCh/IG7NOPPJj7OWk4
mHxE9vN4bRy49axjTx6qFoQbsej79wSCEsLFmqShkwnrIl6OrCthjFivxMSOS7yh
J6twi9CRj466TrcLCOR6ByxzOwKBgQDAhFFwRtC1DazXdsYRmPM8M09CBtxqJKoT
cbr184slP1Uwpm24MOyx21kMWCCtcf3cq0E4p8KGT9K1PSBO1tYP4KK3FxH7WBLu
5PSes1+rBzlPcBm3MBQ+oS2zrKwFIl4zL7oY0IRafyBYJFyYhKcubEf4xDTp0LaP
LsZ2PYU0JQKBgQC6Wb3Q/LiAX7Q9dLiMGPTOg2EFioVIO73NqB14R6GxDOa6K+3n
Hi9ZED2G0heIkDLm+x+hvG6TMLEDJ/PA57XNQ89Cs+qBRxIPvbDK39hqRqPYGB8U
AzTV03+8Nz/LmBxZ4sDYd2K2mB2xhoJ+uC+17Tw8JoG4wHqzTc286fXYiQKBgFlk
V2haCYk4e1xlE12SdscqUBApnWlP8wUqVxTjbGX0PXtknCfd19pSLswTk8DV4Hn5
r0Q+aY3yPLXZV0okaatftE5t0PSzlb8F4yxZv/9rUEo5lEdCdPQL5Wu7rviHV9RO
wgvJrttSklM7Nx++4N4me1jgym0Tc+GSZ6b5tANdAoGBAMgMcaX9zZMxQSVPZXE5
jixW0IcRF+Zas4BkjP7xcohSBN2Lofdf1CpDSxRkggdNea0nx/tluFlPAp987+cj
dO5fM3Sd5b9gGqZyCoNK/p067OFrkHiCy9uCFb6nlcAn5DxKpkVOKnOykylmI0VV
5THqMUBX4MaL5w6zXrmCzzHN
-----END PRIVATE KEY-----"#;
    const TEST_JWK_N: &str = concat!(
        "owP_s6Z1lpQQhzUKGLwGbdofpTkGHx2nd74iFicc2ucb3d_82MOjB8d3KZR_mhLTekx5FYkVlg",
        "NG9fSdBaET0kFddxHLoHkuIiaBehhwaz-WFAUOz7zJ1TS4EfeBnNDg2tRNff6T2kYyk7AMXAHskSpaAL",
        "HJr58xoxmMD1wFVTXo6jcTBOmvkb11xf0RrX0PmoHQURFomJvEt6n_Gzm6Awy58XeEWVT6wZPLzyLo_Q",
        "rYeMEaM9ebfu4YDVwiUrGQKGdgc-uCATeB0Dh_dfpbhRzOIgwpbp4qnsxAs-fwB7PwSrfJ3aKuCWxa3q",
        "2nbW48458RCT5QFNqHA9_W_Xujhw"
    );
    const TEST_JWK_E: &str = "AQAB";

    #[test]
    fn extracts_roles_from_string_or_array_claims() {
        let mut extra = serde_json::Map::new();
        extra.insert(
            "roles".to_owned(),
            serde_json::json!(["lumenhorizon.admin", "reader"]),
        );

        assert_eq!(
            roles_from_claim(&extra, "roles"),
            vec!["lumenhorizon.admin".to_owned(), "reader".to_owned()]
        );

        extra.insert("roles".to_owned(), serde_json::json!("lumenhorizon.admin"));
        assert_eq!(
            roles_from_claim(&extra, "roles"),
            vec!["lumenhorizon.admin".to_owned()]
        );
    }

    #[test]
    fn validates_matching_tenant_claim_when_configured() {
        let config = auth_config(Some("11111111-1111-1111-1111-111111111111"));
        let claims = claims_with_tenant(Some("11111111-1111-1111-1111-111111111111"));

        validate_registered_claims(&config, &claims).unwrap();
    }

    #[test]
    fn rejects_missing_tenant_claim_when_configured() {
        let config = auth_config(Some("11111111-1111-1111-1111-111111111111"));
        let claims = claims_with_tenant(None);
        let error = validate_registered_claims(&config, &claims).unwrap_err();

        assert_eq!(error.status, StatusCode::UNAUTHORIZED);
        assert_eq!(error.message, "invalid token tenant");
    }

    #[test]
    fn rejects_wrong_tenant_claim_when_configured() {
        let config = auth_config(Some("11111111-1111-1111-1111-111111111111"));
        let claims = claims_with_tenant(Some("22222222-2222-2222-2222-222222222222"));
        let error = validate_registered_claims(&config, &claims).unwrap_err();

        assert_eq!(error.status, StatusCode::UNAUTHORIZED);
        assert_eq!(error.message, "invalid token tenant");
    }

    #[tokio::test]
    async fn authenticates_valid_rs256_token_from_jwks_fixture() {
        let jwks_url = spawn_jwks_server(valid_jwks_body()).await;
        let service = AuthService::new(auth_config_with_jwks(&jwks_url, Some(TEST_TENANT_ID)));
        let token = signed_rs256_token(Some(TEST_KID), valid_token_claims());
        let admin = service
            .authenticate(&authorization_headers(&token))
            .await
            .unwrap();

        assert_eq!(admin.subject, "admin-subject");
        assert_eq!(admin.roles, vec!["lumenhorizon.admin".to_owned()]);
    }

    #[tokio::test]
    async fn rejects_rs256_token_with_wrong_issuer() {
        let jwks_url = spawn_jwks_server(valid_jwks_body()).await;
        let service = AuthService::new(auth_config_with_jwks(&jwks_url, Some(TEST_TENANT_ID)));
        let mut claims = valid_token_claims();
        claims.iss = "https://login.microsoftonline.com/other/v2.0".to_owned();
        let error = service
            .authenticate(&authorization_headers(&signed_rs256_token(
                Some(TEST_KID),
                claims,
            )))
            .await
            .unwrap_err();

        assert_eq!(error.status, StatusCode::UNAUTHORIZED);
        assert_eq!(error.message, "invalid token issuer");
    }

    #[tokio::test]
    async fn rejects_rs256_token_with_wrong_audience() {
        let jwks_url = spawn_jwks_server(valid_jwks_body()).await;
        let service = AuthService::new(auth_config_with_jwks(&jwks_url, Some(TEST_TENANT_ID)));
        let mut claims = valid_token_claims();
        claims.aud = json!(["api://other-audience"]);
        let error = service
            .authenticate(&authorization_headers(&signed_rs256_token(
                Some(TEST_KID),
                claims,
            )))
            .await
            .unwrap_err();

        assert_eq!(error.status, StatusCode::UNAUTHORIZED);
        assert_eq!(error.message, "invalid token audience");
    }

    #[tokio::test]
    async fn rejects_rs256_token_with_wrong_tenant() {
        let jwks_url = spawn_jwks_server(valid_jwks_body()).await;
        let service = AuthService::new(auth_config_with_jwks(&jwks_url, Some(TEST_TENANT_ID)));
        let mut claims = valid_token_claims();
        claims.tid = Some("22222222-2222-2222-2222-222222222222".to_owned());
        let error = service
            .authenticate(&authorization_headers(&signed_rs256_token(
                Some(TEST_KID),
                claims,
            )))
            .await
            .unwrap_err();

        assert_eq!(error.status, StatusCode::UNAUTHORIZED);
        assert_eq!(error.message, "invalid token tenant");
    }

    #[tokio::test]
    async fn rejects_expired_rs256_token() {
        let jwks_url = spawn_jwks_server(valid_jwks_body()).await;
        let service = AuthService::new(auth_config_with_jwks(&jwks_url, Some(TEST_TENANT_ID)));
        let now = super::unix_timestamp();
        let mut claims = valid_token_claims();
        claims.iat = now.saturating_sub(900);
        claims.exp = now.saturating_sub(120);
        let error = service
            .authenticate(&authorization_headers(&signed_rs256_token(
                Some(TEST_KID),
                claims,
            )))
            .await
            .unwrap_err();

        assert_eq!(error.status, StatusCode::UNAUTHORIZED);
        assert_eq!(error.message, "token is expired");
    }

    #[tokio::test]
    async fn rejects_rs256_token_with_future_not_before() {
        let jwks_url = spawn_jwks_server(valid_jwks_body()).await;
        let service = AuthService::new(auth_config_with_jwks(&jwks_url, Some(TEST_TENANT_ID)));
        let now = super::unix_timestamp();
        let mut claims = valid_token_claims();
        claims.nbf = Some(now + 300);
        let error = service
            .authenticate(&authorization_headers(&signed_rs256_token(
                Some(TEST_KID),
                claims,
            )))
            .await
            .unwrap_err();

        assert_eq!(error.status, StatusCode::UNAUTHORIZED);
        assert_eq!(error.message, "token is not yet valid");
    }

    #[tokio::test]
    async fn rejects_rs256_token_with_excessive_lifetime() {
        let jwks_url = spawn_jwks_server(valid_jwks_body()).await;
        let service = AuthService::new(auth_config_with_jwks(&jwks_url, Some(TEST_TENANT_ID)));
        let now = super::unix_timestamp();
        let mut claims = valid_token_claims();
        claims.iat = now.saturating_sub(60);
        claims.exp = claims.iat + 3_601;
        let error = service
            .authenticate(&authorization_headers(&signed_rs256_token(
                Some(TEST_KID),
                claims,
            )))
            .await
            .unwrap_err();

        assert_eq!(error.status, StatusCode::UNAUTHORIZED);
        assert_eq!(error.message, "token lifetime is too long");
    }

    #[tokio::test]
    async fn rejects_rs256_token_without_admin_role() {
        let jwks_url = spawn_jwks_server(valid_jwks_body()).await;
        let service = AuthService::new(auth_config_with_jwks(&jwks_url, Some(TEST_TENANT_ID)));
        let mut claims = valid_token_claims();
        claims.roles = Some(json!(["reader"]));
        let error = service
            .authenticate(&authorization_headers(&signed_rs256_token(
                Some(TEST_KID),
                claims,
            )))
            .await
            .unwrap_err();

        assert_eq!(error.status, StatusCode::FORBIDDEN);
        assert_eq!(error.message, "admin role is required");
    }

    #[tokio::test]
    async fn rejects_unsupported_token_algorithm_before_jwks_lookup() {
        let service = AuthService::new(auth_config_with_jwks(
            "http://127.0.0.1:9/unused",
            Some(TEST_TENANT_ID),
        ));
        let mut header = Header::new(Algorithm::HS256);
        header.kid = Some(TEST_KID.to_owned());
        let token = encode(
            &header,
            &valid_token_claims(),
            &EncodingKey::from_secret(b"not-rsa"),
        )
        .unwrap();
        let error = service
            .authenticate(&authorization_headers(&token))
            .await
            .unwrap_err();

        assert_eq!(error.status, StatusCode::UNAUTHORIZED);
        assert_eq!(error.message, "unsupported token algorithm");
    }

    #[tokio::test]
    async fn rejects_rs256_token_without_key_id() {
        let service = AuthService::new(auth_config_with_jwks(
            "http://127.0.0.1:9/unused",
            Some(TEST_TENANT_ID),
        ));
        let error = service
            .authenticate(&authorization_headers(&signed_rs256_token(
                None,
                valid_token_claims(),
            )))
            .await
            .unwrap_err();

        assert_eq!(error.status, StatusCode::UNAUTHORIZED);
        assert_eq!(error.message, "token key id is missing");
    }

    #[tokio::test]
    async fn rejects_rs256_token_with_unknown_key_id_after_jwks_refresh() {
        let jwks_url = spawn_jwks_server(valid_jwks_body()).await;
        let service = AuthService::new(auth_config_with_jwks(&jwks_url, Some(TEST_TENANT_ID)));
        let error = service
            .authenticate(&authorization_headers(&signed_rs256_token(
                Some("unknown-key"),
                valid_token_claims(),
            )))
            .await
            .unwrap_err();

        assert_eq!(error.status, StatusCode::UNAUTHORIZED);
        assert_eq!(error.message, "token signing key is unknown");
    }

    #[tokio::test]
    async fn rejects_invalid_jwks_response() {
        let jwks_url = spawn_jwks_server("{\"keys\":[{\"kid\":\"broken\"}]}".to_owned()).await;
        let service = AuthService::new(auth_config_with_jwks(&jwks_url, Some(TEST_TENANT_ID)));
        let error = service
            .authenticate(&authorization_headers(&signed_rs256_token(
                Some(TEST_KID),
                valid_token_claims(),
            )))
            .await
            .unwrap_err();

        assert_eq!(error.status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(error.message, "JWKS response is invalid");
    }

    fn auth_config(tenant_id: Option<&str>) -> AuthConfig {
        auth_config_with_jwks(
            "https://login.microsoftonline.com/test/discovery/v2.0/keys",
            tenant_id,
        )
    }

    fn auth_config_with_jwks(jwks_url: &str, tenant_id: Option<&str>) -> AuthConfig {
        AuthConfig {
            issuer: "https://login.microsoftonline.com/test/v2.0".to_owned(),
            audience: "api://lumenhorizon-admin".to_owned(),
            jwks_url: jwks_url.to_owned(),
            tenant_id: tenant_id.map(ToOwned::to_owned),
            admin_role_claim: "roles".to_owned(),
            admin_required_role: "lumenhorizon.admin".to_owned(),
            jwks_cache_ttl: Duration::from_secs(300),
            jwt_clock_skew: Duration::from_secs(60),
            max_admin_token_lifetime: Duration::from_secs(3600),
        }
    }

    fn claims_with_tenant(tenant_id: Option<&str>) -> Claims {
        let now = super::unix_timestamp();
        Claims {
            sub: "admin-subject".to_owned(),
            tid: tenant_id.map(ToOwned::to_owned),
            exp: now + 600,
            iat: now,
            extra: serde_json::Map::new(),
        }
    }

    async fn spawn_jwks_server(body: String) -> String {
        let body = Arc::new(body);
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let app = Router::new().route(
            "/keys",
            get({
                let body = Arc::clone(&body);
                move || {
                    let body = Arc::clone(&body);
                    async move {
                        (
                            [(
                                header::CONTENT_TYPE,
                                HeaderValue::from_static("application/json"),
                            )],
                            body.as_str().to_owned(),
                        )
                            .into_response()
                    }
                }
            }),
        );

        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        format!("{}://{address}/keys", "http")
    }

    fn valid_jwks_body() -> String {
        json!({
            "keys": [
                {
                    "kid": TEST_KID,
                    "kty": "RSA",
                    "n": TEST_JWK_N,
                    "e": TEST_JWK_E
                }
            ]
        })
        .to_string()
    }

    fn authorization_headers(token: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {token}")).unwrap(),
        );
        headers
    }

    fn signed_rs256_token(kid: Option<&str>, claims: TestTokenClaims) -> String {
        let mut header = Header::new(Algorithm::RS256);
        header.kid = kid.map(ToOwned::to_owned);
        encode(
            &header,
            &claims,
            &EncodingKey::from_rsa_pem(TEST_PRIVATE_KEY_PEM.as_bytes()).unwrap(),
        )
        .unwrap()
    }

    fn valid_token_claims() -> TestTokenClaims {
        let now = super::unix_timestamp();
        TestTokenClaims {
            iss: "https://login.microsoftonline.com/test/v2.0".to_owned(),
            aud: json!(["api://lumenhorizon-admin", "api://secondary"]),
            sub: "admin-subject".to_owned(),
            tid: Some(TEST_TENANT_ID.to_owned()),
            exp: now + 600,
            iat: now,
            nbf: Some(now.saturating_sub(1)),
            roles: Some(json!(["lumenhorizon.admin"])),
        }
    }

    #[derive(Debug, Serialize)]
    struct TestTokenClaims {
        iss: String,
        aud: Value,
        sub: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        tid: Option<String>,
        exp: u64,
        iat: u64,
        #[serde(skip_serializing_if = "Option::is_none")]
        nbf: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        roles: Option<Value>,
    }
}
