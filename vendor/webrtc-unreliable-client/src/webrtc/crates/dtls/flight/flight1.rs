use super::flight3::*;
use super::*;
use crate::webrtc::dtls::compression_methods::*;
use crate::webrtc::dtls::config::*;
use crate::webrtc::dtls::conn::*;
use crate::webrtc::dtls::content::*;
use crate::webrtc::dtls::curve::named_curve::*;
use crate::webrtc::dtls::error::Error;
use crate::webrtc::dtls::extension::extension_server_name::*;
use crate::webrtc::dtls::extension::extension_supported_elliptic_curves::*;
use crate::webrtc::dtls::extension::extension_supported_point_formats::*;
use crate::webrtc::dtls::extension::extension_supported_signature_algorithms::*;
use crate::webrtc::dtls::extension::extension_use_extended_master_secret::*;
use crate::webrtc::dtls::extension::*;
use crate::webrtc::dtls::handshake::handshake_message_client_hello::*;
use crate::webrtc::dtls::handshake::*;
use crate::webrtc::dtls::record_layer::record_layer_header::*;
use crate::webrtc::dtls::record_layer::*;

use crate::webrtc::dtls::extension::renegotiation_info::ExtensionRenegotiationInfo;
use async_trait::async_trait;
use std::fmt;
use std::sync::atomic::Ordering;

#[derive(Debug, PartialEq)]
pub(crate) struct Flight1;

impl fmt::Display for Flight1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Flight 1")
    }
}

#[async_trait]
impl Flight for Flight1 {
    async fn parse(
        &self,
        tx: &mut channel::Sender<channel::Sender<()>>,
        state: &mut State,
        cache: &HandshakeCache,
        cfg: &HandshakeConfig,
    ) -> Result<Box<dyn Flight + Send + Sync>, (Option<Alert>, Option<Error>)> {
        // HelloVerifyRequest can be skipped by the server,
        // so allow ServerHello during flight1 also
        let (seq, msgs) = match cache
            .full_pull_map(
                state.handshake_recv_sequence,
                &[
                    HandshakeCachePullRule {
                        typ: HandshakeType::HelloVerifyRequest,
                        epoch: cfg.initial_epoch,
                        is_client: false,
                        optional: true,
                    },
                    HandshakeCachePullRule {
                        typ: HandshakeType::ServerHello,
                        epoch: cfg.initial_epoch,
                        is_client: false,
                        optional: true,
                    },
                ],
            )
            .await
        {
            // No valid message received. Keep reading
            Ok((seq, msgs)) => (seq, msgs),
            Err(_) => return Err((None, None)),
        };

        if msgs.contains_key(&HandshakeType::ServerHello) {
            // Flight1 and flight2 were skipped.
            // Parse as flight3.
            let flight3 = Flight3 {};
            return flight3.parse(tx, state, cache, cfg).await;
        }

        if let Some(message) = msgs.get(&HandshakeType::HelloVerifyRequest) {
            // DTLS 1.2 clients must not assume that the server will use the protocol version
            // specified in HelloVerifyRequest message. RFC 6347 Section 4.2.1
            let h = match message {
                HandshakeMessage::HelloVerifyRequest(h) => h,
                _ => {
                    return Err((
                        Some(Alert {
                            alert_level: AlertLevel::Fatal,
                            alert_description: AlertDescription::InternalError,
                        }),
                        None,
                    ))
                }
            };

            if h.version != PROTOCOL_VERSION1_0 && h.version != PROTOCOL_VERSION1_2 {
                return Err((
                    Some(Alert {
                        alert_level: AlertLevel::Fatal,
                        alert_description: AlertDescription::ProtocolVersion,
                    }),
                    Some(Error::ErrUnsupportedProtocolVersion),
                ));
            }

            state.cookie = h.cookie.clone();
            state.handshake_recv_sequence = seq;
            Ok(Box::new(Flight3 {}))
        } else {
            Err((
                Some(Alert {
                    alert_level: AlertLevel::Fatal,
                    alert_description: AlertDescription::InternalError,
                }),
                None,
            ))
        }
    }

    async fn generate(
        &self,
        state: &mut State,
        _cache: &HandshakeCache,
        cfg: &HandshakeConfig,
    ) -> Result<Vec<Packet>, (Option<Alert>, Option<Error>)> {
        let zero_epoch = 0;
        state.local_epoch.store(zero_epoch, Ordering::SeqCst);
        state.remote_epoch.store(zero_epoch, Ordering::SeqCst);

        state.named_curve = DEFAULT_NAMED_CURVE;
        state.cookie = vec![];
        state.local_random.populate();

        let mut extensions = vec![
            Extension::SupportedSignatureAlgorithms(ExtensionSupportedSignatureAlgorithms {
                signature_hash_algorithms: cfg.local_signature_schemes.clone(),
            }),
            Extension::RenegotiationInfo(ExtensionRenegotiationInfo {
                renegotiated_connection: 0,
            }),
        ];

        extensions.extend_from_slice(&[
            Extension::SupportedEllipticCurves(ExtensionSupportedEllipticCurves {
                elliptic_curves: vec![NamedCurve::P256, NamedCurve::X25519, NamedCurve::P384],
            }),
            Extension::SupportedPointFormats(ExtensionSupportedPointFormats {
                point_formats: vec![ELLIPTIC_CURVE_POINT_FORMAT_UNCOMPRESSED],
            }),
        ]);

        if cfg.extended_master_secret == ExtendedMasterSecretType::Request
            || cfg.extended_master_secret == ExtendedMasterSecretType::Require
        {
            extensions.push(Extension::UseExtendedMasterSecret(
                ExtensionUseExtendedMasterSecret { supported: true },
            ));
        }

        if !cfg.server_name.is_empty() {
            extensions.push(Extension::ServerName(ExtensionServerName {
                server_name: cfg.server_name.clone(),
            }));
        }

        Ok(vec![Packet {
            record: RecordLayer::new(
                PROTOCOL_VERSION1_2,
                0,
                Content::Handshake(Handshake::new(HandshakeMessage::ClientHello(
                    HandshakeMessageClientHello {
                        version: PROTOCOL_VERSION1_2,
                        random: state.local_random.clone(),
                        cookie: state.cookie.clone(),

                        cipher_suites: cfg.local_cipher_suites.clone(),
                        compression_methods: default_compression_methods(),
                        extensions,
                    },
                ))),
            ),
            should_encrypt: false,
        }])
    }
}
