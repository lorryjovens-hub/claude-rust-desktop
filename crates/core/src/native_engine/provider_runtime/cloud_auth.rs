//! Cloud credentials are resolved in memory and never passed to child processes.
use anyhow::{bail, Context, Result};
use base64::Engine;
use reqwest::header::{HeaderMap, HeaderValue};
use ring::{digest, hmac, rand::SystemRandom, signature::RsaKeyPair};
use serde_json::{json, Value};

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn sign(key: &[u8], text: &str) -> Vec<u8> {
    hmac::sign(&hmac::Key::new(hmac::HMAC_SHA256, key), text.as_bytes())
        .as_ref()
        .to_vec()
}

pub(super) fn aws_headers(url: &str, region: &str, body: &str) -> Result<HeaderMap> {
    let (access, secret, token) = aws_credentials()?;
    let now = chrono::Utc::now();
    aws_sign(
        url,
        region,
        body,
        &access,
        &secret,
        token.as_deref(),
        &now.format("%Y%m%dT%H%M%SZ").to_string(),
    )
}

fn aws_sign(
    url: &str,
    region: &str,
    body: &str,
    access: &str,
    secret: &str,
    token: Option<&str>,
    timestamp: &str,
) -> Result<HeaderMap> {
    let url = reqwest::Url::parse(url)?;
    let host = match url.port() {
        Some(port) => format!(
            "{}:{port}",
            url.host_str().context("AWS endpoint has no host")?
        ),
        None => url
            .host_str()
            .context("AWS endpoint has no host")?
            .to_string(),
    };
    let hash = hex(digest::digest(&digest::SHA256, body.as_bytes()).as_ref());
    let mut headers = HeaderMap::new();
    headers.insert("host", HeaderValue::from_str(&host)?);
    headers.insert("content-type", HeaderValue::from_static("application/json"));
    headers.insert("x-amz-date", HeaderValue::from_str(timestamp)?);
    headers.insert("x-amz-content-sha256", HeaderValue::from_str(&hash)?);
    if let Some(token) = token {
        headers.insert("x-amz-security-token", HeaderValue::from_str(token)?);
    }
    let sorted = headers
        .iter()
        .map(|(k, v)| Ok((k.as_str().to_owned(), v.to_str()?.trim().to_owned())))
        .collect::<Result<std::collections::BTreeMap<_, _>>>()?;
    let signed = sorted.keys().cloned().collect::<Vec<_>>().join(";");
    let canonical_headers = sorted
        .iter()
        .map(|(k, v)| format!("{k}:{v}\n"))
        .collect::<String>();
    // Bedrock requires RFC3986 encoding of the model ID, including ARN separators.
    let canonical_path = url
        .path()
        .split('/')
        .map(|part| urlencoding::encode(part).into_owned())
        .collect::<Vec<_>>()
        .join("/");
    let canonical = format!(
        "POST\n{}\n{}\n{}\n{}\n{}",
        canonical_path,
        url.query().unwrap_or_default(),
        canonical_headers,
        signed,
        hash
    );
    let date = &timestamp[..8];
    let scope = format!("{date}/{region}/bedrock/aws4_request");
    let to_sign = format!(
        "AWS4-HMAC-SHA256\n{timestamp}\n{scope}\n{}",
        hex(digest::digest(&digest::SHA256, canonical.as_bytes()).as_ref())
    );
    let key = sign(format!("AWS4{secret}").as_bytes(), date);
    let key = sign(&key, region);
    let key = sign(&key, "bedrock");
    let key = sign(&key, "aws4_request");
    let signature = hex(&sign(&key, &to_sign));
    headers.insert("authorization",HeaderValue::from_str(&format!("AWS4-HMAC-SHA256 Credential={access}/{scope}, SignedHeaders={signed}, Signature={signature}"))?);
    Ok(headers)
}

fn aws_credentials() -> Result<(String, String, Option<String>)> {
    let env = |name| std::env::var(name).ok().filter(|s| !s.is_empty());
    if std::env::var_os("AWS_PROFILE").is_none() {
        if let (Some(access), Some(secret)) =
            (env("AWS_ACCESS_KEY_ID"), env("AWS_SECRET_ACCESS_KEY"))
        {
            return Ok((access, secret, env("AWS_SESSION_TOKEN")));
        }
    }
    let path = std::env::var_os("AWS_SHARED_CREDENTIALS_FILE")
        .map(std::path::PathBuf::from)
        .or_else(|| dirs::home_dir().map(|p| p.join(".aws/credentials")))
        .context("AWS credentials path unavailable")?;
    let profile = env("AWS_PROFILE").unwrap_or_else(|| "default".into());
    let mut selected = false;
    let mut fields = std::collections::HashMap::new();
    if let Ok(content) = std::fs::read_to_string(path) {
        for line in content.lines().map(str::trim) {
            if line.starts_with('[') && line.ends_with(']') {
                selected = line[1..line.len() - 1] == profile;
            } else if selected && !line.starts_with('#') && !line.starts_with(';') {
                if let Some((key, value)) = line.split_once('=') {
                    fields.insert(key.trim().to_string(), value.trim().to_string());
                }
            }
        }
    }
    let access = fields.remove("aws_access_key_id");
    let secret = fields.remove("aws_secret_access_key");
    match (access,secret) {
        (Some(access),Some(secret))=>Ok((access,secret,fields.remove("aws_session_token"))),
        _=>bail!("Bedrock requires AWS_BEARER_TOKEN_BEDROCK, AWS access-key environment variables, or a shared credentials profile. Role/SSO/container credential chains are not yet supported by this native adapter"),
    }
}

pub(super) async fn google_token(client: &reqwest::Client) -> Result<String> {
    if let Ok(token) = std::env::var("GOOGLE_OAUTH_ACCESS_TOKEN") {
        if !token.is_empty() {
            return Ok(token);
        }
    }
    let path = std::env::var_os("GOOGLE_APPLICATION_CREDENTIALS")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            dirs::config_dir().map(|p| p.join("gcloud/application_default_credentials.json"))
        })
        .context("Google ADC credentials path unavailable")?;
    let credentials: Value = serde_json::from_str(&std::fs::read_to_string(path).context("Vertex requires GOOGLE_CLOUD_API_KEY, GOOGLE_OAUTH_ACCESS_TOKEN, or an ADC credentials file")?)?;
    let response = match credentials["type"].as_str() {
        Some("authorized_user") => {
            client.post("https://oauth2.googleapis.com/token").form(&[
                ("grant_type","refresh_token"),("client_id",credentials["client_id"].as_str().context("ADC client ID missing")?),
                ("client_secret",credentials["client_secret"].as_str().context("ADC client secret missing")?),
                ("refresh_token",credentials["refresh_token"].as_str().context("ADC refresh token missing")?),
            ]).send().await?
        }
        Some("service_account") => {
            let email = credentials["client_email"].as_str().context("Service account email missing")?;
            let pem = credentials["private_key"].as_str().context("Service account key missing")?;
            let encoded = pem.lines().filter(|l| !l.starts_with("---")).collect::<String>();
            let der = base64::engine::general_purpose::STANDARD.decode(encoded).context("Invalid service account key encoding")?;
            let key = RsaKeyPair::from_pkcs8(&der).map_err(|_|anyhow::anyhow!("Invalid service account RSA key"))?;
            let now = chrono::Utc::now().timestamp();
            let b64 = &base64::engine::general_purpose::URL_SAFE_NO_PAD;
            let header = b64.encode(br#"{"alg":"RS256","typ":"JWT"}"#);
            let claims = b64.encode(json!({"iss":email,"scope":"https://www.googleapis.com/auth/cloud-platform","aud":"https://oauth2.googleapis.com/token","iat":now,"exp":now+3600}).to_string());
            let signing_input = format!("{header}.{claims}");
            let mut signature = vec![0;key.public().modulus_len()];
            key.sign(&ring::signature::RSA_PKCS1_SHA256,&SystemRandom::new(),signing_input.as_bytes(),&mut signature).map_err(|_|anyhow::anyhow!("Service account JWT signing failed"))?;
            let assertion = format!("{signing_input}.{}",b64.encode(signature));
            client.post("https://oauth2.googleapis.com/token").form(&[("grant_type","urn:ietf:params:oauth:grant-type:jwt-bearer"),("assertion",assertion.as_str())]).send().await?
        }
        _=>bail!("Unsupported ADC credential type; native Vertex currently supports service_account and authorized_user credentials"),
    };
    if !response.status().is_success() {
        bail!(
            "Google credential refresh failed with HTTP {}",
            response.status()
        );
    }
    let token: Value = response.json().await?;
    token["access_token"]
        .as_str()
        .map(str::to_string)
        .context("Google credential response has no access token")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sigv4_includes_payload_and_session_token_and_is_deterministic() {
        let h = aws_sign(
            "https://bedrock-runtime.us-east-1.amazonaws.com/model/test/converse-stream",
            "us-east-1",
            "{}",
            "AKID",
            "SECRET",
            Some("TOKEN"),
            "20260101T000000Z",
        )
        .unwrap();
        assert_eq!(
            h["x-amz-content-sha256"],
            "44136fa355b3678a1146ad16f7e8649e94fb4fc21fe77e8310c060f61caaff8a"
        );
        assert_eq!(h["x-amz-security-token"], "TOKEN");
        assert!(h["authorization"]
            .to_str()
            .unwrap()
            .contains("20260101/us-east-1/bedrock/aws4_request"));
        assert_ne!(
            h["authorization"],
            aws_sign(
                "https://bedrock-runtime.us-east-1.amazonaws.com/model/test/converse-stream",
                "us-east-1",
                "{\"a\":1}",
                "AKID",
                "SECRET",
                Some("TOKEN"),
                "20260101T000000Z"
            )
            .unwrap()["authorization"]
        );
    }
}
