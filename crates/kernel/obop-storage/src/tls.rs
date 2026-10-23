//! TLS for PostgreSQL connections (D66): rustls (no system OpenSSL), chosen in the database URL.
//!
//! `sslmode=disable|prefer|require|verify-ca|verify-full` and an optional `sslrootcert=PATH`
//! (or `system`) are read from the query of a `postgres://` URL. A URL without `sslmode` keeps
//! the behaviour it always had: a plain connection. Every PostgreSQL connection of the product
//! goes through [`PgConnector`], so the choice is made once, in one place.

use postgres::tls::{ChannelBinding, MakeTlsConnect, TlsConnect, TlsStream};
use postgres::{Client, Config, NoTls};
use rustls::client::WebPkiServerVerifier;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{
    CertificateError, ClientConfig, DigitallySignedStruct, Error as TlsError, RootCertStore,
    SignatureScheme,
};
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio_rustls::TlsConnector;

/// How far a connection is protected, in libpq's words.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SslMode {
    /// No TLS. The default when the URL says nothing.
    Disable,
    /// Use TLS when the server offers it, otherwise a plain connection; the certificate is not checked.
    Prefer,
    /// TLS is required; the certificate is not checked (protects against eavesdropping only).
    Require,
    /// TLS is required and the certificate chain must lead to a trusted root; the name is not checked.
    VerifyCa,
    /// TLS is required, the chain must be trusted and the certificate must name the host.
    VerifyFull,
}

impl SslMode {
    pub fn as_str(self) -> &'static str {
        match self {
            SslMode::Disable => "disable",
            SslMode::Prefer => "prefer",
            SslMode::Require => "require",
            SslMode::VerifyCa => "verify-ca",
            SslMode::VerifyFull => "verify-full",
        }
    }
    fn parse(s: &str) -> Result<Self, String> {
        Ok(match s {
            "disable" => SslMode::Disable,
            "prefer" => SslMode::Prefer,
            "require" => SslMode::Require,
            "verify-ca" => SslMode::VerifyCa,
            "verify-full" => SslMode::VerifyFull,
            other => {
                return Err(format!(
                    "unsupported sslmode {other:?} in the database URL (use disable, prefer, require, verify-ca or verify-full)"
                ));
            }
        })
    }
}

/// Where the trusted roots come from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RootCert {
    /// A PEM file of one or more certificates (`sslrootcert=/path/ca.pem`).
    File(String),
    /// The bundled Mozilla roots (`sslrootcert=system`, and the default for `verify-*`).
    Bundled,
}

/// What the URL asked for, with the TLS parameters removed from the connection string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    pub conn: String,
    pub mode: SslMode,
    pub root_cert: Option<RootCert>,
}

fn pct_decode(s: &str) -> Result<String, String> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let hex = s
                .get(i + 1..i + 3)
                .ok_or("bad percent escape in the database URL")?;
            out.push(
                u8::from_str_radix(hex, 16)
                    .map_err(|_| "bad percent escape in the database URL")?,
            );
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).map_err(|_| "the database URL is not valid UTF-8".to_string())
}

/// Read `sslmode` and `sslrootcert` from the query of a `postgres://` URL. Everything else is
/// passed on untouched. A URL that does not mention them yields [`SslMode::Disable`].
pub fn parse_url(url: &str) -> Result<Settings, String> {
    if !url.starts_with("postgres://") && !url.starts_with("postgresql://") {
        return Err("the database address must be a postgres:// URL".into());
    }
    let (base, query) = match url.split_once('?') {
        Some((b, q)) => (b, q),
        None => (url, ""),
    };
    let mut mode: Option<SslMode> = None;
    let mut root: Option<RootCert> = None;
    let mut rest: Vec<&str> = Vec::new();
    for pair in query.split('&').filter(|p| !p.is_empty()) {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        match k {
            "sslmode" => {
                if mode.is_some() {
                    return Err("sslmode appears twice in the database URL".into());
                }
                mode = Some(SslMode::parse(&pct_decode(v)?)?);
            }
            "sslrootcert" => {
                if root.is_some() {
                    return Err("sslrootcert appears twice in the database URL".into());
                }
                let v = pct_decode(v)?;
                if v.is_empty() {
                    return Err("sslrootcert is empty in the database URL".into());
                }
                root = Some(if v == "system" {
                    RootCert::Bundled
                } else {
                    RootCert::File(v)
                });
            }
            "sslcert" | "sslkey" | "sslcrl" | "sslsni" | "sslcompression" | "sslpassword" => {
                return Err(format!(
                    "{k} in the database URL is not supported (client certificates are not built)"
                ));
            }
            _ => rest.push(pair),
        }
    }
    let mode = mode.unwrap_or(SslMode::Disable);
    if root.is_some()
        && !matches!(
            mode,
            SslMode::Require | SslMode::VerifyCa | SslMode::VerifyFull
        )
    {
        return Err(
            "sslrootcert needs sslmode=require, verify-ca or verify-full in the database URL"
                .into(),
        );
    }
    let conn = if rest.is_empty() {
        base.to_string()
    } else {
        format!("{base}?{}", rest.join("&"))
    };
    Ok(Settings {
        conn,
        mode,
        root_cert: root,
    })
}

fn load_roots(root: &Option<RootCert>) -> Result<RootCertStore, String> {
    let mut store = RootCertStore::empty();
    match root {
        None | Some(RootCert::Bundled) => {
            store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        }
        Some(RootCert::File(path)) => {
            let mut n = 0;
            for cert in CertificateDer::pem_file_iter(path)
                .map_err(|e| format!("cannot read sslrootcert {path}: {e}"))?
            {
                let cert = cert.map_err(|e| format!("sslrootcert {path} is not valid PEM: {e}"))?;
                store
                    .add(cert)
                    .map_err(|e| format!("sslrootcert {path} holds a bad certificate: {e}"))?;
                n += 1;
            }
            if n == 0 {
                return Err(format!("sslrootcert {path} holds no certificate"));
            }
        }
    }
    Ok(store)
}

/// Accepts any certificate (the `prefer` and `require` modes) but still checks that the server
/// holds the key of the certificate it sent, so the handshake itself is sound.
#[derive(Debug)]
struct NoVerify(Arc<rustls::crypto::CryptoProvider>);

impl ServerCertVerifier for NoVerify {
    fn verify_server_cert(
        &self,
        _: &CertificateDer<'_>,
        _: &[CertificateDer<'_>],
        _: &ServerName<'_>,
        _: &[u8],
        _: UnixTime,
    ) -> Result<ServerCertVerified, TlsError> {
        Ok(ServerCertVerified::assertion())
    }
    fn verify_tls12_signature(
        &self,
        m: &[u8],
        c: &CertificateDer<'_>,
        d: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        rustls::crypto::verify_tls12_signature(m, c, d, &self.0.signature_verification_algorithms)
    }
    fn verify_tls13_signature(
        &self,
        m: &[u8],
        c: &CertificateDer<'_>,
        d: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        rustls::crypto::verify_tls13_signature(m, c, d, &self.0.signature_verification_algorithms)
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}

/// `verify-ca`: the full path validation, except that a certificate that does not name the
/// host is accepted (the libpq meaning).
#[derive(Debug)]
struct ChainOnly(Arc<WebPkiServerVerifier>);

impl ServerCertVerifier for ChainOnly {
    fn verify_server_cert(
        &self,
        end: &CertificateDer<'_>,
        inter: &[CertificateDer<'_>],
        name: &ServerName<'_>,
        ocsp: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, TlsError> {
        match self.0.verify_server_cert(end, inter, name, ocsp, now) {
            Err(TlsError::InvalidCertificate(
                CertificateError::NotValidForName | CertificateError::NotValidForNameContext { .. },
            )) => Ok(ServerCertVerified::assertion()),
            other => other,
        }
    }
    fn verify_tls12_signature(
        &self,
        m: &[u8],
        c: &CertificateDer<'_>,
        d: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        self.0.verify_tls12_signature(m, c, d)
    }
    fn verify_tls13_signature(
        &self,
        m: &[u8],
        c: &CertificateDer<'_>,
        d: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        self.0.verify_tls13_signature(m, c, d)
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.0.supported_verify_schemes()
    }
}

fn client_config(settings: &Settings) -> Result<ClientConfig, String> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let builder = ClientConfig::builder_with_provider(provider.clone())
        .with_safe_default_protocol_versions()
        .map_err(|e| format!("TLS setup failed: {e}"))?;
    // libpq: `require` with a root certificate on file is checked like `verify-ca`.
    let mode = match (settings.mode, &settings.root_cert) {
        (SslMode::Require, Some(_)) => SslMode::VerifyCa,
        (m, _) => m,
    };
    Ok(match mode {
        SslMode::Disable | SslMode::Prefer | SslMode::Require => builder
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(NoVerify(provider)))
            .with_no_client_auth(),
        SslMode::VerifyCa => {
            let roots = load_roots(&settings.root_cert)?;
            let inner = WebPkiServerVerifier::builder_with_provider(Arc::new(roots), provider)
                .build()
                .map_err(|e| format!("TLS setup failed: {e}"))?;
            builder
                .dangerous()
                .with_custom_certificate_verifier(Arc::new(ChainOnly(inner)))
                .with_no_client_auth()
        }
        SslMode::VerifyFull => builder
            .with_root_certificates(load_roots(&settings.root_cert)?)
            .with_no_client_auth(),
    })
}

/// A TLS stream in the shape the PostgreSQL driver wants. There is no channel binding, so the
/// server never offers `SCRAM-SHA-256-PLUS` to this client; plain SCRAM-SHA-256 is used.
pub struct PgTlsStream<S>(tokio_rustls::client::TlsStream<S>);

impl<S: AsyncRead + AsyncWrite + Unpin> AsyncRead for PgTlsStream<S> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.0).poll_read(cx, buf)
    }
}

impl<S: AsyncRead + AsyncWrite + Unpin> AsyncWrite for PgTlsStream<S> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.0).poll_write(cx, buf)
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.0).poll_flush(cx)
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.0).poll_shutdown(cx)
    }
}

impl<S: AsyncRead + AsyncWrite + Unpin> TlsStream for PgTlsStream<S> {
    fn channel_binding(&self) -> ChannelBinding {
        ChannelBinding::none()
    }
}

pub struct Connect {
    connector: TlsConnector,
    name: ServerName<'static>,
}

type ConnectFuture<S> = Pin<Box<dyn Future<Output = io::Result<PgTlsStream<S>>> + Send>>;

impl<S: AsyncRead + AsyncWrite + Unpin + Send + 'static> TlsConnect<S> for Connect {
    type Stream = PgTlsStream<S>;
    type Error = io::Error;
    type Future = ConnectFuture<S>;
    fn connect(self, stream: S) -> Self::Future {
        Box::pin(async move {
            let s = self.connector.connect(self.name, stream).await?;
            Ok(PgTlsStream(s))
        })
    }
}

/// Builds the TLS side of a connection for the host being dialled.
#[derive(Clone)]
pub struct Maker(TlsConnector);

impl<S: AsyncRead + AsyncWrite + Unpin + Send + 'static> MakeTlsConnect<S> for Maker {
    type Stream = PgTlsStream<S>;
    type TlsConnect = Connect;
    type Error = io::Error;
    fn make_tls_connect(&mut self, domain: &str) -> Result<Connect, io::Error> {
        let name = ServerName::try_from(domain.to_string())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid server name"))?;
        Ok(Connect {
            connector: self.0.clone(),
            name,
        })
    }
}

/// A parsed database URL ready to open connections: used for every PostgreSQL connection.
#[derive(Clone)]
pub struct PgConnector {
    config: Config,
    mode: SslMode,
    maker: Option<Maker>,
}

impl PgConnector {
    /// Parse the URL, read the root certificate file (if any) and prepare the TLS settings.
    /// Errors never include the URL, so a password cannot reach a log through them.
    pub fn new(url: &str) -> Result<Self, String> {
        let settings = parse_url(url)?;
        let mut config: Config = settings
            .conn
            .parse()
            .map_err(|e: postgres::Error| format!("invalid PostgreSQL database URL: {e}"))?;
        let maker = match settings.mode {
            SslMode::Disable => None,
            _ => Some(Maker(TlsConnector::from(Arc::new(client_config(
                &settings,
            )?)))),
        };
        config.ssl_mode(match settings.mode {
            SslMode::Disable => postgres::config::SslMode::Disable,
            SslMode::Prefer => postgres::config::SslMode::Prefer,
            _ => postgres::config::SslMode::Require,
        });
        Ok(Self {
            config,
            mode: settings.mode,
            maker,
        })
    }

    pub fn mode(&self) -> SslMode {
        self.mode
    }

    /// The driver configuration, for callers that set their own options (a connect timeout).
    pub fn config_mut(&mut self) -> &mut Config {
        &mut self.config
    }

    pub fn connect(&self) -> Result<Client, String> {
        match &self.maker {
            None => self.config.connect(NoTls),
            Some(m) => self.config.connect(m.clone()),
        }
        .map_err(|e| e.to_string())
    }
}

/// One-shot: open a connection for `url` (used by tools that connect once).
pub fn connect(url: &str) -> Result<Client, String> {
    PgConnector::new(url)?.connect()
}
