use super::flight3::*;
use super::*;
use crate::webrtc::dtls::change_cipher_spec::ChangeCipherSpec;
use crate::webrtc::dtls::content::*;
use crate::webrtc::dtls::crypto::*;
use crate::webrtc::dtls::curve::named_curve::*;
use crate::webrtc::dtls::curve::*;
use crate::webrtc::dtls::error::Error;
use crate::webrtc::dtls::handshake::handshake_message_client_key_exchange::*;
use crate::webrtc::dtls::handshake::handshake_message_finished::*;
use crate::webrtc::dtls::handshake::handshake_message_server_key_exchange::*;
use crate::webrtc::dtls::handshake::*;
use crate::webrtc::dtls::prf::*;
use crate::webrtc::dtls::record_layer::record_layer_header::*;
use crate::webrtc::dtls::record_layer::*;
use crate::webrtc::dtls::signature_hash_algorithm::*;

use async_trait::async_trait;
use std::fmt;
use std::io::{BufReader, BufWriter};

#[derive(Debug, PartialEq)]
pub(crate) struct Flight5;

impl fmt::Display for Flight5 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Flight 5")
    }
}

#[async_trait]
impl Flight for Flight5 {
    fn is_last_recv_flight(&self) -> bool {
        true
    }

    async fn parse(
        &self,
        _tx: &mut channel::Sender<channel::Sender<()>>,
        state: &mut State,
        cache: &HandshakeCache,
        cfg: &HandshakeConfig,
    ) -> Result<Box<dyn Flight + Send + Sync>, (Option<Alert>, Option<Error>)> {
        let (_seq, msgs) = match cache
            .full_pull_map(
                state.handshake_recv_sequence,
                &[HandshakeCachePullRule {
                    typ: HandshakeType::Finished,
                    epoch: cfg.initial_epoch + 1,
                    is_client: false,
                    optional: false,
                }],
            )
            .await
        {
            Ok((seq, msgs)) => (seq, msgs),
            Err(_) => return Err((None, None)),
        };

        let finished =
            if let Some(HandshakeMessage::Finished(h)) = msgs.get(&HandshakeType::Finished) {
                h
            } else {
                return Err((
                    Some(Alert {
                        alert_level: AlertLevel::Fatal,
                        alert_description: AlertDescription::InternalError,
                    }),
                    None,
                ));
            };

        let plain_text = cache
            .pull_and_merge(&[
                HandshakeCachePullRule {
                    typ: HandshakeType::ClientHello,
                    epoch: cfg.initial_epoch,
                    is_client: true,
                    optional: false,
                },
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
                    optional: false,
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
                HandshakeCachePullRule {
                    typ: HandshakeType::Certificate,
                    epoch: cfg.initial_epoch,
                    is_client: true,
                    optional: false,
                },
                HandshakeCachePullRule {
                    typ: HandshakeType::ClientKeyExchange,
                    epoch: cfg.initial_epoch,
                    is_client: true,
                    optional: false,
                },
                HandshakeCachePullRule {
                    typ: HandshakeType::CertificateVerify,
                    epoch: cfg.initial_epoch,
                    is_client: true,
                    optional: false,
                },
                HandshakeCachePullRule {
                    typ: HandshakeType::Finished,
                    epoch: cfg.initial_epoch + 1,
                    is_client: true,
                    optional: false,
                },
            ])
            .await;

        {
            let cipher_suite = state.cipher_suite.lock().await;
            if let Some(cipher_suite) = &*cipher_suite {
                let expected_verify_data = match prf_verify_data_server(
                    &state.master_secret,
                    &plain_text,
                    cipher_suite.hash_func(),
                ) {
                    Ok(d) => d,
                    Err(err) => {
                        return Err((
                            Some(Alert {
                                alert_level: AlertLevel::Fatal,
                                alert_description: AlertDescription::InsufficientSecurity,
                            }),
                            Some(err),
                        ))
                    }
                };

                if expected_verify_data != finished.verify_data {
                    return Err((
                        Some(Alert {
                            alert_level: AlertLevel::Fatal,
                            alert_description: AlertDescription::HandshakeFailure,
                        }),
                        Some(Error::ErrVerifyDataMismatch),
                    ));
                }
            }
        }

        Ok(Box::new(Flight5 {}))
    }

    async fn generate(
        &self,
        state: &mut State,
        cache: &HandshakeCache,
        cfg: &HandshakeConfig,
    ) -> Result<Vec<Packet>, (Option<Alert>, Option<Error>)> {
        let mut pkts = vec![];

        let mut client_key_exchange = HandshakeMessageClientKeyExchange {
            identity_hint: vec![],
            public_key: vec![],
        };
        if let Some(local_keypair) = &state.local_keypair {
            client_key_exchange.public_key = local_keypair.public_key.clone();
        }

        pkts.push(Packet {
            record: RecordLayer::new(
                PROTOCOL_VERSION1_2,
                0,
                Content::Handshake(Handshake::new(HandshakeMessage::ClientKeyExchange(
                    client_key_exchange,
                ))),
            ),
            should_encrypt: false,
        });

        let server_key_exchange_data = cache
            .pull_and_merge(&[HandshakeCachePullRule {
                typ: HandshakeType::ServerKeyExchange,
                epoch: cfg.initial_epoch,
                is_client: false,
                optional: false,
            }])
            .await;

        let mut server_key_exchange = HandshakeMessageServerKeyExchange {
            identity_hint: vec![],
            elliptic_curve_type: EllipticCurveType::Unsupported,
            named_curve: NamedCurve::Unsupported,
            public_key: vec![],
            algorithm: SignatureHashAlgorithm {
                hash: HashAlgorithm::Unsupported,
                signature: SignatureAlgorithm::Unsupported,
            },
            signature: vec![],
        };

        // handshakeMessageServerKeyExchange is optional for PSK
        if server_key_exchange_data.is_empty() {
            if let Err((alert, err)) = handle_server_key_exchange(state, cfg, &server_key_exchange)
            {
                return Err((alert, err));
            }
        } else {
            let mut reader = BufReader::new(server_key_exchange_data.as_slice());
            let raw_handshake = match Handshake::unmarshal(&mut reader) {
                Ok(h) => h,
                Err(err) => {
                    return Err((
                        Some(Alert {
                            alert_level: AlertLevel::Fatal,
                            alert_description: AlertDescription::UnexpectedMessage,
                        }),
                        Some(err),
                    ))
                }
            };

            match raw_handshake.handshake_message {
                HandshakeMessage::ServerKeyExchange(h) => server_key_exchange = h,
                _ => {
                    return Err((
                        Some(Alert {
                            alert_level: AlertLevel::Fatal,
                            alert_description: AlertDescription::UnexpectedMessage,
                        }),
                        Some(Error::ErrInvalidContentType),
                    ))
                }
            };
        }

        // Append not-yet-sent packets
        let mut merged = vec![];
        for (seq_pred, p) in (state.handshake_send_sequence as u16..).zip(pkts.iter_mut()) {
            let h = match &mut p.record.content {
                Content::Handshake(h) => h,
                _ => {
                    return Err((
                        Some(Alert {
                            alert_level: AlertLevel::Fatal,
                            alert_description: AlertDescription::InternalError,
                        }),
                        Some(Error::ErrInvalidContentType),
                    ))
                }
            };
            h.handshake_header.message_sequence = seq_pred;

            let mut raw = vec![];
            {
                let mut writer = BufWriter::<&mut Vec<u8>>::new(raw.as_mut());
                if let Err(err) = h.marshal(&mut writer) {
                    return Err((
                        Some(Alert {
                            alert_level: AlertLevel::Fatal,
                            alert_description: AlertDescription::InternalError,
                        }),
                        Some(err),
                    ));
                }
            }

            merged.extend_from_slice(&raw);
        }

        if let Err((alert, err)) =
            initalize_cipher_suite(state, cache, cfg, &server_key_exchange, &merged).await
        {
            return Err((alert, err));
        }

        pkts.push(Packet {
            record: RecordLayer::new(
                PROTOCOL_VERSION1_2,
                0,
                Content::ChangeCipherSpec(ChangeCipherSpec {}),
            ),
            should_encrypt: false,
        });

        if state.local_verify_data.is_empty() {
            let mut plain_text = cache
                .pull_and_merge(&[
                    HandshakeCachePullRule {
                        typ: HandshakeType::ClientHello,
                        epoch: cfg.initial_epoch,
                        is_client: true,
                        optional: false,
                    },
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
                        optional: false,
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
                    HandshakeCachePullRule {
                        typ: HandshakeType::Certificate,
                        epoch: cfg.initial_epoch,
                        is_client: true,
                        optional: false,
                    },
                    HandshakeCachePullRule {
                        typ: HandshakeType::ClientKeyExchange,
                        epoch: cfg.initial_epoch,
                        is_client: true,
                        optional: false,
                    },
                    HandshakeCachePullRule {
                        typ: HandshakeType::CertificateVerify,
                        epoch: cfg.initial_epoch,
                        is_client: true,
                        optional: false,
                    },
                    HandshakeCachePullRule {
                        typ: HandshakeType::Finished,
                        epoch: cfg.initial_epoch + 1,
                        is_client: true,
                        optional: false,
                    },
                ])
                .await;

            plain_text.extend_from_slice(&merged);

            let cipher_suite = state.cipher_suite.lock().await;
            if let Some(cipher_suite) = &*cipher_suite {
                state.local_verify_data = match prf_verify_data_client(
                    &state.master_secret,
                    &plain_text,
                    cipher_suite.hash_func(),
                ) {
                    Ok(data) => data,
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
            }
        }

        pkts.push(Packet {
            record: RecordLayer::new(
                PROTOCOL_VERSION1_2,
                1,
                Content::Handshake(Handshake::new(HandshakeMessage::Finished(
                    HandshakeMessageFinished {
                        verify_data: state.local_verify_data.clone(),
                    },
                ))),
            ),
            should_encrypt: true,
        });

        Ok(pkts)
    }
}
async fn initalize_cipher_suite(
    state: &mut State,
    cache: &HandshakeCache,
    cfg: &HandshakeConfig,
    h: &HandshakeMessageServerKeyExchange,
    sending_plain_text: &[u8],
) -> Result<(), (Option<Alert>, Option<Error>)> {
    let mut cipher_suite = state.cipher_suite.lock().await;

    if let Some(cipher_suite) = &*cipher_suite {
        if cipher_suite.is_initialized() {
            return Ok(());
        }
    }

    let mut client_random = vec![];
    {
        let mut writer = BufWriter::<&mut Vec<u8>>::new(client_random.as_mut());
        let _ = state.local_random.marshal(&mut writer);
    }
    let mut server_random = vec![];
    {
        let mut writer = BufWriter::<&mut Vec<u8>>::new(server_random.as_mut());
        let _ = state.remote_random.marshal(&mut writer);
    }

    if let Some(cipher_suite) = &*cipher_suite {
        if state.extended_master_secret {
            let session_hash = match cache
                .session_hash(
                    cipher_suite.hash_func(),
                    cfg.initial_epoch,
                    sending_plain_text,
                )
                .await
            {
                Ok(s) => s,
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

            state.master_secret = match prf_extended_master_secret(
                &state.pre_master_secret,
                &session_hash,
                cipher_suite.hash_func(),
            ) {
                Ok(m) => m,
                Err(err) => {
                    return Err((
                        Some(Alert {
                            alert_level: AlertLevel::Fatal,
                            alert_description: AlertDescription::IllegalParameter,
                        }),
                        Some(err),
                    ))
                }
            };
        } else {
            state.master_secret = match prf_master_secret(
                &state.pre_master_secret,
                &client_random,
                &server_random,
                cipher_suite.hash_func(),
            ) {
                Ok(m) => m,
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
        }
    }

    // Verify that the pair of hash algorithm and signature is listed.
    let mut valid_signature_scheme = false;
    for ss in &cfg.local_signature_schemes {
        if ss.hash == h.algorithm.hash && ss.signature == h.algorithm.signature {
            valid_signature_scheme = true;
            break;
        }
    }
    if !valid_signature_scheme {
        return Err((
            Some(Alert {
                alert_level: AlertLevel::Fatal,
                alert_description: AlertDescription::InsufficientSecurity,
            }),
            Some(Error::ErrNoAvailableSignatureSchemes),
        ));
    }

    // The server key exchange signature is still checked against the
    // certificate the server presented; WebRTC then authenticates that
    // certificate by its SDP fingerprint, not a webpki chain.
    let expected_msg =
        value_key_message(&client_random, &server_random, &h.public_key, h.named_curve);
    if let Err(err) = verify_key_signature(
        &expected_msg,
        &h.algorithm,
        &h.signature,
        &state.peer_certificates,
    ) {
        return Err((
            Some(Alert {
                alert_level: AlertLevel::Fatal,
                alert_description: AlertDescription::BadCertificate,
            }),
            Some(err),
        ));
    }

    if let Some(cipher_suite) = &mut *cipher_suite {
        if let Err(err) =
            cipher_suite.init(&state.master_secret, &client_random, &server_random, true)
        {
            return Err((
                Some(Alert {
                    alert_level: AlertLevel::Fatal,
                    alert_description: AlertDescription::InternalError,
                }),
                Some(err),
            ));
        }
    }

    Ok(())
}
