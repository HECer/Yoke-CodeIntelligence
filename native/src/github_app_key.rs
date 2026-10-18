//! Separate opaque app-key capability. Never serialized, hashed or formatted.
use anyhow::{Result, ensure};
use base64::{
    Engine as _,
    prelude::{BASE64_STANDARD, BASE64_URL_SAFE_NO_PAD},
};
use ring::{
    rand::SystemRandom,
    signature::{RSA_PKCS1_SHA256, RsaKeyPair},
};
use zeroize::Zeroizing;

const INVALID: &str = "github_app_key_invalid";
pub struct AppKey(RsaKeyPair);
fn parse(der: &[u8], pkcs8: Option<bool>) -> Result<RsaKeyPair> {
    let key = match pkcs8 {
        Some(true) => RsaKeyPair::from_pkcs8(der),
        Some(false) => RsaKeyPair::from_der(der),
        None => RsaKeyPair::from_pkcs8(der).or_else(|_| RsaKeyPair::from_der(der)),
    }
    .map_err(|_| anyhow::anyhow!(INVALID))?;
    ensure!((256..=512).contains(&key.public().modulus_len()), INVALID);
    Ok(key)
}
pub(crate) fn decoded_key(raw: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
    ensure!(!raw.is_empty() && raw.len() <= 8192, INVALID);
    let (der, pkcs8) = if raw.starts_with(b"-----") {
        let text = std::str::from_utf8(raw).map_err(|_| anyhow::anyhow!(INVALID))?;
        ensure!(text.is_ascii(), INVALID);
        let text = text.trim_matches(|c| matches!(c, '\r' | '\n'));
        let mut selected = None;
        for (label, pkcs8) in [("RSA PRIVATE KEY", false), ("PRIVATE KEY", true)] {
            let begin = format!("-----BEGIN {label}-----");
            let end = format!("-----END {label}-----");
            if let Some(body) = text.strip_prefix(&begin).and_then(|s| s.strip_suffix(&end)) {
                let encoded = Zeroizing::new(
                    body.bytes()
                        .filter(|b| !matches!(b, b'\r' | b'\n'))
                        .collect::<Vec<_>>(),
                );
                let der = Zeroizing::new(
                    BASE64_STANDARD
                        .decode(&*encoded)
                        .map_err(|_| anyhow::anyhow!(INVALID))?,
                );
                selected = Some((der, Some(pkcs8)));
                break;
            }
        }
        selected.ok_or_else(|| anyhow::anyhow!(INVALID))?
    } else {
        (Zeroizing::new(raw.to_vec()), None)
    };
    ensure!(der.len() <= 2560, INVALID);
    parse(&der, pkcs8)?;
    Ok(der)
}
impl AppKey {
    pub fn from_host_bytes(raw: &[u8]) -> Result<Self> {
        let der = decoded_key(raw)?;
        Ok(Self(parse(&der, None)?))
    }
    pub(super) fn jwt(&self, issuer: &str, now: u64) -> Result<Zeroizing<String>> {
        ensure!(
            (60..=253_402_296_000).contains(&now),
            "github_app_clock_invalid"
        );
        let claims = serde_json::json!({"iat":now-60,"exp":now+540,"iss":issuer});
        let header = BASE64_URL_SAFE_NO_PAD.encode(b"{\"alg\":\"RS256\",\"typ\":\"JWT\"}");
        let payload = BASE64_URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims)?);
        let mut token = Zeroizing::new(format!("{header}.{payload}"));
        let mut signature = Zeroizing::new(vec![0; self.0.public().modulus_len()]);
        self.0
            .sign(
                &RSA_PKCS1_SHA256,
                &SystemRandom::new(),
                token.as_bytes(),
                &mut signature,
            )
            .map_err(|_| anyhow::anyhow!("github_app_signing_failed"))?;
        token.push('.');
        token.push_str(&BASE64_URL_SAFE_NO_PAD.encode(&*signature));
        Ok(token)
    }
}
