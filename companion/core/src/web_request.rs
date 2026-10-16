// Makes a web request the user set up (the notes URL, remote buttons) for the desktop app.
//
// It mirrors node/web-request.js: extra headers and a login arrive ready in the headers, and an https
// address's certificate is checked the usual way, against a pinned SHA-256 fingerprint for a
// self-signed certificate, or not at all. The check happens during the handshake, so nothing is sent to
// a server that fails it.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::sync::Arc;
use std::time::Duration;

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{verify_tls12_signature, verify_tls13_signature, CryptoProvider};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, ClientConnection, DigitallySignedStruct, SignatureScheme, StreamOwned};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use ureq::config::RedirectAuthHeaders;
use ureq::unversioned::resolver::DefaultResolver;
use ureq::unversioned::transport::{
    Buffers, ConnectionDetails, Connector, Either, LazyBuffers, NextTimeout, TcpConnector, Transport,
    TransportAdapter,
};

// The most of a reply kept, so a huge page cannot fill memory.
const MOST_REPLY_BYTES: u64 = 4 * 1024 * 1024;

// ============================================================================
// Requests and replies
// ============================================================================

// Which certificate an https address must show.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CertificateTrust {
    System,
    Pinned,
    Any,
}

// A request as src/services/web-request.ts sends it.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebRequest {
    pub method: String,
    pub url: String,
    pub headers: BTreeMap<String, String>,
    pub body: Option<String>,
    pub timeout_ms: u64,
    pub certificate: CertificateTrust,
    pub certificate_sha256: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WebReply {
    pub status: u16,
    pub status_text: String,
    pub body: Vec<u8>,
}

// The fingerprint as 32 bytes, whether written with colons, spaces or neither.
fn fingerprint_bytes(text: &str) -> Result<[u8; 32], String> {
    let digits: String = text.chars().filter(|c| !c.is_whitespace() && *c != ':').collect();
    let mut bytes = [0u8; 32];
    if digits.len() != 64 {
        return Err("A SHA-256 fingerprint has 64 hex digits.".into());
    }
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&digits[index * 2..index * 2 + 2], 16)
            .map_err(|_| "A SHA-256 fingerprint has only hex digits.".to_string())?;
    }
    Ok(bytes)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub fn request(request: &WebRequest) -> Result<WebReply, String> {
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_millis(request.timeout_ms)))
        .http_status_as_error(false)
        .redirect_auth_headers(RedirectAuthHeaders::SameHost)
        .build();
    let agent = match request.certificate {
        CertificateTrust::System => ureq::Agent::new_with_config(config),
        trust => {
            let pin = match trust {
                CertificateTrust::Pinned => Some(fingerprint_bytes(
                    request.certificate_sha256.as_deref().unwrap_or_default(),
                )?),
                _ => None,
            };
            let connector = ().chain(TcpConnector::default()).chain(TrustingTls::new(pin));
            ureq::Agent::with_parts(config, connector, DefaultResolver::default())
        }
    };

    let method: ureq::http::Method = request
        .method
        .parse()
        .map_err(|_| format!("{} is not an HTTP method.", request.method))?;
    let mut builder = ureq::http::Request::builder().method(method).uri(&request.url);
    for (name, value) in &request.headers {
        builder = builder.header(name, value);
    }
    let outgoing = builder
        .body(request.body.clone().unwrap_or_default())
        .map_err(|error| error.to_string())?;
    let mut reply = agent.run(outgoing).map_err(|error| match error {
        ureq::Error::Timeout(_) => format!("No answer within {} s.", request.timeout_ms / 1000),
        other => other.to_string(),
    })?;
    let status = reply.status();
    let body = reply
        .body_mut()
        .with_config()
        .limit(MOST_REPLY_BYTES)
        .read_to_vec()
        .map_err(|error| error.to_string())?;
    Ok(WebReply {
        status: status.as_u16(),
        status_text: status.canonical_reason().unwrap_or_default().to_string(),
        body,
    })
}

// ============================================================================
// Trusting a pinned certificate, or any
// ============================================================================

// Accepts the server's certificate when its SHA-256 matches the pin, or any certificate without one;
// the handshake's signatures are still checked, so only the certificate's key holder gets through.
#[derive(Debug)]
struct PinnedVerifier {
    pin: Option<[u8; 32]>,
    provider: Arc<CryptoProvider>,
}

impl ServerCertVerifier for PinnedVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let Some(pin) = self.pin else {
            return Ok(ServerCertVerified::assertion());
        };
        let shown: [u8; 32] = Sha256::digest(end_entity.as_ref()).into();
        if shown == pin {
            Ok(ServerCertVerified::assertion())
        } else {
            Err(rustls::Error::General(format!(
                "the server showed a certificate with SHA-256 fingerprint {}, not the pinned one",
                hex(&shown)
            )))
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

// Wraps a connection in TLS checked by `PinnedVerifier`, in place of ureq's own TLS.
#[derive(Debug)]
struct TrustingTls {
    config: Arc<ClientConfig>,
}

impl TrustingTls {
    fn new(pin: Option<[u8; 32]>) -> Self {
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let config = ClientConfig::builder_with_provider(provider.clone())
            .with_safe_default_protocol_versions()
            .expect("the ring provider supports the default TLS versions")
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(PinnedVerifier { pin, provider }))
            .with_no_client_auth();
        TrustingTls {
            config: Arc::new(config),
        }
    }
}

impl<In: Transport> Connector<In> for TrustingTls {
    type Out = Either<In, TrustingTlsTransport>;

    fn connect(
        &self,
        details: &ConnectionDetails,
        chained: Option<In>,
    ) -> Result<Option<Self::Out>, ureq::Error> {
        let Some(transport) = chained else {
            return Ok(None);
        };
        if !details.needs_tls() || transport.is_tls() {
            return Ok(Some(Either::A(transport)));
        }
        let host = details
            .uri
            .host()
            .unwrap_or_default()
            .trim_start_matches('[')
            .trim_end_matches(']');
        let name = ServerName::try_from(host.to_string())
            .map_err(|_| ureq::Error::Tls("the address names no valid host"))?;
        let mut connection = ClientConnection::new(self.config.clone(), name)
            .map_err(|error| ureq::Error::Io(std::io::Error::other(error)))?;
        let mut socket = TransportAdapter::new(transport.boxed());
        socket.set_timeout(details.timeout);
        connection.complete_io(&mut socket)?;
        let buffers = LazyBuffers::new(
            details.config.input_buffer_size(),
            details.config.output_buffer_size(),
        );
        Ok(Some(Either::B(TrustingTlsTransport {
            buffers,
            stream: StreamOwned::new(connection, socket),
        })))
    }
}

struct TrustingTlsTransport {
    buffers: LazyBuffers,
    stream: StreamOwned<ClientConnection, TransportAdapter>,
}

impl std::fmt::Debug for TrustingTlsTransport {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("TrustingTlsTransport").finish()
    }
}

impl Transport for TrustingTlsTransport {
    fn buffers(&mut self) -> &mut dyn Buffers {
        &mut self.buffers
    }

    fn transmit_output(&mut self, amount: usize, timeout: NextTimeout) -> Result<(), ureq::Error> {
        self.stream.get_mut().set_timeout(timeout);
        let output = &self.buffers.output()[..amount];
        self.stream.write_all(output)?;
        Ok(())
    }

    fn await_input(&mut self, timeout: NextTimeout) -> Result<bool, ureq::Error> {
        self.stream.get_mut().set_timeout(timeout);
        let input = self.buffers.input_append_buf();
        let amount = self.stream.read(input)?;
        self.buffers.input_appended(amount);
        Ok(amount > 0)
    }

    fn is_open(&mut self) -> bool {
        self.stream.get_mut().get_mut().is_open()
    }

    fn is_tls(&self) -> bool {
        true
    }
}

// ============================================================================
// How to reach an address, as the companion's settings keep it
// ============================================================================

// Everything about reaching an address besides the address itself, as src/services/web-request.ts keeps it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebAccess {
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub certificate: Option<CertificateTrust>,
    // The pinned certificate's SHA-256 fingerprint, as 64 lowercase hex digits.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub certificate_sha256: Option<String>,
}

impl WebAccess {
    // The access settings in stored data, dropping anything malformed, as readWebAccess does.
    pub fn read(value: &serde_json::Value) -> WebAccess {
        let mut access = WebAccess::default();
        let Some(stored) = value.as_object() else {
            return access;
        };
        if let Some(headers) = stored.get("headers").and_then(|headers| headers.as_object()) {
            for (name, item) in headers {
                if let Some(text) = item.as_str() {
                    access.headers.insert(name.clone(), text.to_string());
                }
            }
        }
        let filled_text = |key: &str| {
            stored
                .get(key)
                .and_then(|item| item.as_str())
                .filter(|text| !text.is_empty())
                .map(str::to_string)
        };
        access.username = filled_text("username");
        access.password = filled_text("password");
        let fingerprint = stored
            .get("certificateSha256")
            .and_then(|item| item.as_str())
            .and_then(normalized_fingerprint);
        match stored.get("certificate").and_then(|item| item.as_str()) {
            Some("any") => access.certificate = Some(CertificateTrust::Any),
            Some("pinned") if fingerprint.is_some() => {
                access.certificate = Some(CertificateTrust::Pinned);
                access.certificate_sha256 = fingerprint;
            }
            _ => {}
        }
        access
    }

    // The headers a request sends: the user's own, plus an Authorization header when a login is set.
    pub fn headers(&self) -> BTreeMap<String, String> {
        let mut headers = self.headers.clone();
        if self.username.is_some() || self.password.is_some() {
            let login = format!(
                "{}:{}",
                self.username.as_deref().unwrap_or_default(),
                self.password.as_deref().unwrap_or_default()
            );
            headers.insert(
                "Authorization".into(),
                format!("Basic {}", base64(login.as_bytes())),
            );
        }
        headers
    }

    // Checks the address and gives the request to make, as nativeRequest does.
    pub fn request(
        &self,
        method: &str,
        url: &str,
        body: Option<String>,
        timeout_ms: u64,
    ) -> Result<WebRequest, String> {
        let lowercase = url.to_ascii_lowercase();
        if !lowercase.starts_with("http://") && !lowercase.starts_with("https://") {
            return Err(format!("{url} is not an http or https URL."));
        }
        let certificate = self.certificate.unwrap_or(CertificateTrust::System);
        let certificate_sha256 = match certificate {
            CertificateTrust::Pinned => Some(
                self.certificate_sha256
                    .clone()
                    .ok_or("Give the SHA-256 fingerprint of the certificate to trust.")?,
            ),
            _ => None,
        };
        Ok(WebRequest {
            method: method.to_string(),
            url: url.to_string(),
            headers: self.headers(),
            body,
            timeout_ms,
            certificate,
            certificate_sha256,
        })
    }
}

// The fingerprint as 64 lowercase hex digits, whether written with colons, spaces or neither.
pub fn normalized_fingerprint(text: &str) -> Option<String> {
    let digits: String = text
        .chars()
        .filter(|character| !character.is_whitespace() && *character != ':')
        .collect::<String>()
        .to_lowercase();
    let well_formed = digits.len() == 64 && digits.chars().all(|character| character.is_ascii_hexdigit());
    well_formed.then_some(digits)
}

fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for group in bytes.chunks(3) {
        let joined = group.iter().enumerate().fold(0u32, |joined, (place, byte)| {
            joined | (*byte as u32) << (16 - 8 * place)
        });
        for place in 0..4 {
            if place <= group.len() {
                out.push(ALPHABET[(joined >> (18 - 6 * place) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    use std::net::TcpListener;
    use std::path::PathBuf;

    use rustls::pki_types::pem::PemObject;
    use rustls::pki_types::PrivateKeyDer;
    use rustls::{ServerConfig, ServerConnection};

    // The test certificate in node/testdata, made with openssl for localhost and 127.0.0.1.
    const PINNED: &str = "296bae6abfcd699ccb20a5a7465321a23a62785e5be1b7f3697d6bd437dc2415";

    fn testdata(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../node/testdata")
            .join(name)
    }

    // Serves https on a free port with the self-signed test certificate, answering each request with
    // its own Authorization header, and gives the port and how many requests got through the handshake.
    fn serve() -> (u16, Arc<std::sync::atomic::AtomicUsize>) {
        let cert = CertificateDer::from_pem_file(testdata("self-signed.crt")).unwrap();
        let key = PrivateKeyDer::from_pem_file(testdata("self-signed.key")).unwrap();
        let config = Arc::new(
            ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
                .with_safe_default_protocol_versions()
                .unwrap()
                .with_no_client_auth()
                .with_single_cert(vec![cert], key)
                .unwrap(),
        );
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let served = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter = served.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let mut connection = ServerConnection::new(config.clone()).unwrap();
                let mut tls = rustls::Stream::new(&mut connection, &mut stream);
                let mut request = Vec::new();
                let mut chunk = [0u8; 1024];
                loop {
                    match tls.read(&mut chunk) {
                        Ok(0) | Err(_) => break,
                        Ok(amount) => {
                            request.extend_from_slice(&chunk[..amount]);
                            if request.windows(4).any(|window| window == b"\r\n\r\n") {
                                break;
                            }
                        }
                    }
                }
                if request.is_empty() {
                    continue;
                }
                counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let text = String::from_utf8_lossy(&request).to_string();
                let auth = text
                    .lines()
                    .find_map(|line| line.strip_prefix("authorization: "))
                    .unwrap_or("none")
                    .to_string();
                let reply = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{auth}",
                    auth.len()
                );
                let _ = tls.write_all(reply.as_bytes());
                let _ = tls.flush();
                connection.send_close_notify();
                let _ = connection.complete_io(&mut stream);
            }
        });
        (port, served)
    }

    fn get(port: u16, certificate: CertificateTrust, pin: Option<&str>) -> Result<WebReply, String> {
        request(&WebRequest {
            method: "GET".into(),
            url: format!("https://127.0.0.1:{port}/note"),
            headers: BTreeMap::from([("Authorization".into(), "Basic bWU6c2VjcmV0".into())]),
            body: None,
            timeout_ms: 5000,
            certificate,
            certificate_sha256: pin.map(str::to_string),
        })
    }

    #[test]
    fn a_self_signed_certificate_is_trusted_only_when_pinned_or_any_is_allowed() {
        let (port, _) = serve();

        assert!(get(port, CertificateTrust::System, None).is_err());

        let pinned = get(port, CertificateTrust::Pinned, Some(PINNED)).unwrap();
        assert_eq!(pinned.status, 200);
        assert_eq!(pinned.body, b"Basic bWU6c2VjcmV0");

        let colons = PINNED
            .as_bytes()
            .chunks(2)
            .map(|pair| std::str::from_utf8(pair).unwrap().to_uppercase())
            .collect::<Vec<_>>()
            .join(":");
        assert_eq!(
            get(port, CertificateTrust::Pinned, Some(&colons)).unwrap().status,
            200
        );
        assert_eq!(get(port, CertificateTrust::Any, None).unwrap().status, 200);
    }

    #[test]
    fn nothing_is_sent_to_a_server_whose_certificate_is_not_the_pinned_one() {
        let (port, served) = serve();

        let error = get(port, CertificateTrust::Pinned, Some(&"0".repeat(64))).unwrap_err();

        assert!(error.contains(PINNED), "{error}");
        assert_eq!(served.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    #[test]
    fn a_request_reads_as_the_window_sends_it() {
        let request: WebRequest = serde_json::from_str(
            r#"{"method":"GET","url":"https://example.org/","headers":{"X-Custom":"yes"},
                "body":null,"timeoutMs":15000,"certificate":"pinned","certificateSha256":"ab"}"#,
        )
        .unwrap();

        assert_eq!(request.headers["X-Custom"], "yes");
        assert_eq!(request.certificate, CertificateTrust::Pinned);
        assert_eq!(request.timeout_ms, 15000);
    }

    #[test]
    fn a_malformed_fingerprint_is_refused() {
        assert!(fingerprint_bytes("abc").is_err());
        assert!(fingerprint_bytes(&"g".repeat(64)).is_err());
    }

    #[test]
    fn stored_access_keeps_only_what_is_well_formed() {
        let access = WebAccess::read(&serde_json::json!({
            "headers": { "X-Key": "1", "X-Number": 2 },
            "username": "me",
            "password": "",
            "certificate": "pinned",
            "certificateSha256": "AB:".repeat(31) + "AB",
        }));

        assert_eq!(access.headers, BTreeMap::from([("X-Key".into(), "1".into())]));
        assert_eq!(access.username.as_deref(), Some("me"));
        assert_eq!(access.password, None);
        assert_eq!(access.certificate, Some(CertificateTrust::Pinned));
        assert_eq!(access.certificate_sha256, Some("ab".repeat(32)));
        let unpinned =
            WebAccess::read(&serde_json::json!({ "certificate": "pinned", "certificateSha256": "ab" }));
        assert_eq!(unpinned.certificate, None);
    }

    #[test]
    fn a_login_becomes_a_basic_authorization_header() {
        let access = WebAccess {
            username: Some("me".into()),
            password: Some("secret".into()),
            ..WebAccess::default()
        };

        assert_eq!(access.headers()["Authorization"], "Basic bWU6c2VjcmV0");
        assert_eq!(base64(b"a"), "YQ==");
        assert_eq!(base64(b"ab"), "YWI=");
        assert_eq!(base64(b"abc"), "YWJj");
    }

    #[test]
    fn only_web_addresses_are_requested() {
        let access = WebAccess::default();

        assert_eq!(
            access
                .request("GET", "ftp://example.org", None, 15000)
                .unwrap_err(),
            "ftp://example.org is not an http or https URL."
        );
        let request = access
            .request("POST", "HTTPS://example.org", Some("x".into()), 15000)
            .unwrap();
        assert_eq!(request.certificate, CertificateTrust::System);
        assert_eq!(request.body.as_deref(), Some("x"));
    }
}
