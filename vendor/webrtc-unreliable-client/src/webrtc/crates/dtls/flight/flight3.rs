use super::flight5::*;
use super::*;
use crate::webrtc::dtls::compression_methods::*;
use crate::webrtc::dtls::config::*;
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
use crate::webrtc::dtls::handshake::handshake_message_server_key_exchange::*;
use crate::webrtc::dtls::handshake::*;
use crate::webrtc::dtls::record_layer::record_layer_header::*;
use crate::webrtc::dtls::record_layer::*;

use crate::webrtc::dtls::cipher_suite::cipher_suite_for_id;
use crate::webrtc::dtls::find_matching_cipher_suite;
use crate::webrtc::dtls::prf::prf_pre_master_secret;

use crate::webrtc::dtls::extension::renegotiation_info::ExtensionRenegotiationInfo;
use async_trait::async_trait;
use log::*;
use std::fmt;

#[derive(Debug, PartialEq)]
pub(crate) struct Flight3;

impl fmt::Display for Flight3 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Flight 3")
    }
}

#[async_trait]
impl Flight for Flight3 {
    async fn parse(
        &self,
        _tx: &mut channel::Sender<channel::Sender<()>>,
        state: &mut State,
        cache: &HandshakeCache,
        cfg: &HandshakeConfig,
    ) -> Result<Box<dyn Flight + Send + Sync>, (Option<Alert>, Option<Error>)> {
        // Clients may receive multiple HelloVerifyRequest messages with different cookies.
        // Clients SHOULD handle this by sending a new ClientHello with a cookie in response
        // to the new HelloVerifyRequest. RFC 6347 Section 4.2.1
        if let Ok((seq, msgs)) = cache
            .full_pull_map(
                state.handshake_recv_sequence,
                &[HandshakeCachePullRule {
                    typ: HandshakeType::HelloVerifyRequest,
                    epoch: cfg.initial_epoch,
                    is_client: false,
                    optional: true,
                }],
            )
            .await
        {
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

                // DTLS 1.2 clients must not assume that the server will use the protocol version
                // specified in HelloVerifyRequest message. RFC 6347 Section 4.2.1
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
                return Ok(Box::new(Flight3 {}) as Box<dyn Flight + Send + Sync>);
            }
        }

        let result = cache
            .full_pull_map(
                state.handshake_recv_sequence,
                &[
                    HandshakeCachePullRule {
                        typ: HandshakeType::ServerHello,
                        epoch: cfg.initial_epoch,
                        is_client: false,
                        optional: false,
                    },
                    HandshakeCachePullRule {
                        typ: HandshakeType::Certificate,
                        epoch: cfg.initial_epoch,
                        is_client: false,
                        optional: true,
                    },
                    HandshakeCachePullRule {
                        typ: HandshakeType::ServerKeyExchange,
                        epoch: cfg.initial_epoch,
                        is_client: false,
                        optional: false,
                    },
                    HandshakeCachePullRule {
                        typ: HandshakeType::ServerHelloDone,
                        epoch: cfg.initial_epoch,
                        is_client: false,
                        optional: false,
                    },
                ],
            )
            .await;

        let (seq, msgs) = match result {
            Ok((seq, msgs)) => (seq, msgs),
            Err(_) => return Err((None, None)),
        };

        state.handshake_recv_sequence = seq;

        if let Some(message) = msgs.get(&HandshakeType::ServerHello) {
            let h = match message {
                HandshakeMessage::ServerHello(h) => h,
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

            if h.version != PROTOCOL_VERSION1_2 {
                return Err((
                    Some(Alert {
                        alert_level: AlertLevel::Fatal,
                        alert_description: AlertDescription::ProtocolVersion,
                    }),
                    Some(Error::ErrUnsupportedProtocolVersion),
                ));
            }

            for extension in &h.extensions {
                if let Extension::UseExtendedMasterSecret(_) = extension {
                    if cfg.extended_master_secret != ExtendedMasterSecretType::Disable {
                        state.extended_master_secret = true;
                    }
                };
            }

            if cfg.extended_master_secret == ExtendedMasterSecretType::Require
                && !state.extended_master_secret
            {
                return Err((
                    Some(Alert {
                        alert_level: AlertLevel::Fatal,
                        alert_description: AlertDescription::InsufficientSecurity,
                    }),
                    Some(Error::ErrClientRequiredButNoServerEms),
                ));
            }
            if find_matching_cipher_suite(&[h.cipher_suite], &cfg.local_cipher_suites).is_err() {
                debug!(
                    "[handshake:{}] use cipher suite: {}",
                    srv_cli_str(state.is_client),
                    h.cipher_suite
                );

                return Err((
                    Some(Alert {
                        alert_level: AlertLevel::Fatal,
                        alert_description: AlertDescription::InsufficientSecurity,
                    }),
                    Some(Error::ErrCipherSuiteNoIntersection),
                ));
            }

            let cipher_suite = match cipher_suite_for_id(h.cipher_suite) {
                Ok(cipher_suite) => cipher_suite,
                Err(_) => {
                    debug!(
                        "[handshake:{}] use cipher suite: {}",
                        srv_cli_str(state.is_client),
                        h.cipher_suite
                    );

                    return Err((
                        Some(Alert {
                            alert_level: AlertLevel::Fatal,
                            alert_description: AlertDescription::InsufficientSecurity,
                        }),
                        Some(Error::ErrInvalidCipherSuite),
                    ));
                }
            };

            trace!(
                "[handshake:{}] use cipher suite: {}",
                srv_cli_str(state.is_client),
                cipher_suite.to_string()
            );
            {
                let mut cs = state.cipher_suite.lock().await;
                *cs = Some(cipher_suite);
            }
            state.remote_random = h.random.clone();
        }

        if let Some(message) = msgs.get(&HandshakeType::Certificate) {
            let h = match message {
                HandshakeMessage::Certificate(h) => h,
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
            state.peer_certificates = h.certificate.clone();
        }

        if let Some(message) = msgs.get(&HandshakeType::ServerKeyExchange) {
            let h = match message {
                HandshakeMessage::ServerKeyExchange(h) => h,
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

            if let Err((alert, err)) = handle_server_key_exchange(state, cfg, h) {
                return Err((alert, err));
            }
        }

        Ok(Box::new(Flight5 {}) as Box<dyn Flight + Send + Sync>)
    }

    async fn generate(
        &self,
        state: &mut State,
        _cache: &HandshakeCache,
        cfg: &HandshakeConfig,
    ) -> Result<Vec<Packet>, (Option<Alert>, Option<Error>)> {
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

pub(crate) fn handle_server_key_exchange(
    state: &mut State,
    _cfg: &HandshakeConfig,
    h: &HandshakeMessageServerKeyExchange,
) -> Result<(), (Option<Alert>, Option<Error>)> {
    {
        let local_keypair = match h.named_curve.generate_keypair() {
            Ok(local_keypair) => local_keypair,
            Err(err) => {
                return Err((
                    Some(Alert {
                        alert_level: AlertLevel::Fatal,
                        alert_description: AlertDescription::InternalError,
                    }),
                    Some(err),
                ))
            }
        };

        state.pre_master_secret = match prf_pre_master_secret(
            &h.public_key,
            &local_keypair.private_key,
            local_keypair.curve,
        ) {
            Ok(pre_master_secret) => pre_master_secret,
            Err(err) => {
                return Err((
                    Some(Alert {
                        alert_level: AlertLevel::Fatal,
                        alert_description: AlertDescription::InternalError,
                    }),
                    Some(err),
                ))
            }
        };

        state.local_keypair = Some(local_keypair);
    }

    Ok(())
}
