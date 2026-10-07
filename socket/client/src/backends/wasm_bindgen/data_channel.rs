extern crate log;

use std::{cell::RefCell, net::SocketAddr, rc::Rc};

use js_sys::{Array, Date, Object, Reflect};
use log::{info, warn};
use tinyjson::JsonValue;
use wasm_bindgen::{closure::Closure, JsCast, JsValue};
use web_sys::{
    ErrorEvent, MessageChannel, MessageEvent, ProgressEvent, RtcConfiguration, RtcDataChannel,
    RtcDataChannelInit, RtcDataChannelState, RtcDataChannelType, RtcIceCandidate,
    RtcIceCandidateInit, RtcIceGatheringState, RtcPeerConnection, RtcPeerConnectionIceEvent,
    RtcSdpType, RtcSessionDescriptionInit, XmlHttpRequest,
};

/// Bound on ICE gathering before the session offer is POSTed. Gathering that
/// never completes is a loud client-side error, never an indefinite hang and
/// never a silent candidate-less offer. Sized generously: typical networks
/// complete in a few seconds, but constrained ones were measured at ~40s of
/// candidate-probing tail before `complete` fires.
const ICE_GATHERING_TIMEOUT_MS: i32 = 60_000;

use naia_socket_shared::{
    parse_server_url, should_post_session_offer, IdentityToken, SocketConfig,
    ICE_GATHER_EARLY_POST_MS,
};

use super::identity_receiver::IdentityReceiver;
use super::{addr_cell::AddrCell, data_port::DataPort};
use crate::ServerAddr;

// FindAddrFuncInner
pub struct FindAddrFuncInner(pub Box<dyn FnMut(SocketAddr)>);

/// Retained handle to one attempt's live WebRTC objects. `start` hands it
/// out so the attempt's sender can tear the connection down on retry: without
/// this the peer survives its attempt (kept alive by forgotten JS closures)
/// and a retried dial shares the wire with a stale peer.
#[derive(Clone)]
pub struct WasmPeerCloser {
    peer: RtcPeerConnection,
    channel: RtcDataChannel,
}

impl WasmPeerCloser {
    /// Closes the data channel first, then the peer. Either close is safe on
    /// an already-closed object, so a repeated shutdown changes nothing.
    pub fn close(self) {
        self.channel.close();
        self.peer.close();
    }
}

/// Drives the WebRTC signaling handshake (session POST, ICE gathering,
/// peer/data-channel setup) for one connection attempt on the wasm_bindgen
/// backend, and hands out the resulting [`AddrCell`], [`DataPort`], and
/// [`IdentityReceiver`].
pub struct DataChannel {
    server_session_url: String,
    auth_bytes_opt: Option<Vec<u8>>,
    /// Every header this request will carry, already stamped with naia's
    /// protocol-fingerprint header by
    /// [`stamp_protocol_id_header`](naia_socket_shared::stamp_protocol_id_header).
    /// Not an `Option`: there is no session request that omits the fingerprint.
    auth_headers: Vec<(String, String)>,
    ice_servers: Vec<String>,
    message_channel: MessageChannel,
    addr_cell: AddrCell,
    id_cell: IdentityReceiver,
    find_addr_func: Rc<RefCell<FindAddrFuncInner>>,
}

impl DataChannel {
    /// Builds a `DataChannel` for one connection attempt, parsing the
    /// session URL and capturing the config's ICE servers. Does not start
    /// signaling; call [`Self::start`] for that.
    pub fn new(
        config: &SocketConfig,
        server_session_url: &str,
        auth_bytes_opt: Option<Vec<u8>>,
        auth_headers: Vec<(String, String)>,
    ) -> Self {
        let server_url = parse_server_url(server_session_url);

        Self {
            server_session_url: format!("{}{}", server_url, config.rtc_endpoint_path.clone()),
            auth_bytes_opt,
            auth_headers,
            ice_servers: config.ice_servers.clone(),
            message_channel: MessageChannel::new().expect("can't create message channel"),
            addr_cell: AddrCell::new(),
            id_cell: IdentityReceiver::new(),
            find_addr_func: Rc::new(RefCell::new(FindAddrFuncInner(Box::new(move |_| {})))),
        }
    }

    /// Returns a clone of the polled view over the server's data-channel
    /// address.
    pub fn addr_cell(&self) -> AddrCell {
        self.addr_cell.clone()
    }

    /// Returns the local half of the message channel used to move packets
    /// between this `DataChannel` and the client's packet sender/receiver.
    pub fn data_port(&self) -> DataPort {
        DataPort::new(self.message_channel.port1())
    }

    /// Returns a clone of the receiver that will yield the identity token,
    /// or rejection, this attempt's signaling produces.
    pub fn id_receiver(&self) -> IdentityReceiver {
        self.id_cell.clone()
    }

    /// Registers the callback invoked once the server's data-channel address
    /// is resolved.
    pub fn on_find_addr(&mut self, func: Box<dyn FnMut(SocketAddr)>) {
        self.find_addr_func
            .as_ref()
            .try_borrow_mut()
            .expect("cannot borrow FindAddrFunc!")
            .0 = func;
    }

    /// Starts the WebRTC signaling handshake: builds the peer connection and
    /// data channel, POSTs the session offer once ICE gathering completes or
    /// the bounded early-post wait passes with a candidate in hand, and wires
    /// up inbound message/identity routing. Returns a closer that can tear
    /// this attempt's peer and channel down on retry.
    #[allow(unused_must_use)]
    pub fn start(&self) -> WasmPeerCloser {
        // Set up Ice Servers from the socket config (defaults to Google's
        // public STUN; override via `SocketConfig.ice_servers`)
        let ice_server_config_urls = Array::new();
        for ice_server in &self.ice_servers {
            ice_server_config_urls.push(&JsValue::from(ice_server));
        }

        let ice_server_config = Object::new();
        Reflect::set(
            &ice_server_config,
            &JsValue::from("urls"),
            &JsValue::from(&ice_server_config_urls),
        );

        let ice_server_config_list = Array::new();
        ice_server_config_list.push(&ice_server_config);

        // Set up RtcConfiguration
        let peer_config: RtcConfiguration = RtcConfiguration::new();
        peer_config.set_ice_servers(&ice_server_config_list);

        // Setup Peer Connection
        match RtcPeerConnection::new_with_configuration(&peer_config) {
            Ok(peer) => {
                let data_channel_config: RtcDataChannelInit = RtcDataChannelInit::new();
                data_channel_config.set_ordered(false);
                data_channel_config.set_max_retransmits(0);

                let channel: RtcDataChannel =
                    peer.create_data_channel_with_data_channel_dict("data", &data_channel_config);
                channel.set_binary_type(RtcDataChannelType::Arraybuffer);

                let onerror_func: Box<dyn FnMut(ErrorEvent)> = Box::new(move |e: ErrorEvent| {
                    info!("data channel error event: {:?}", e);
                });
                let onerror_callback = Closure::wrap(onerror_func);
                channel.set_onerror(Some(onerror_callback.as_ref().unchecked_ref()));
                onerror_callback.forget();

                // The open transition is the usable-link moment (Roger 42320:
                // the channel object exists from connect(), long before
                // ICE/DTLS completes). Nothing observed it on this backend;
                // warn! so the served console (warn-and-above) shows it.
                let onopen_func: Box<dyn FnMut(JsValue)> = Box::new(move |_: JsValue| {
                    warn!("naia: datachannel onopen");
                });
                let onopen_callback = Closure::wrap(onopen_func);
                channel.set_onopen(Some(onopen_callback.as_ref().unchecked_ref()));
                onopen_callback.forget();

                let peer_2 = peer.clone();
                let addr_cell_2 = self.addr_cell.clone();
                let addr_func_2 = self.find_addr_func.clone();
                let id_sender_2 = self.id_cell.clone();
                let server_url_msg = self.server_session_url.clone();
                let auth_bytes_opt_2 = self.auth_bytes_opt.clone();
                let auth_headers_2 = self.auth_headers.clone();
                let peer_offer_func: Box<dyn FnMut(JsValue)> = Box::new(move |e: JsValue| {
                    let session_description = e.into();
                    let peer_3 = peer_2.clone();
                    let addr_cell_3 = addr_cell_2.clone();
                    let addr_func_3 = addr_func_2.clone();
                    let id_sender_3 = id_sender_2.clone();
                    let server_url_msg_2 = server_url_msg.clone();
                    let auth_bytes_opt_3 = auth_bytes_opt_2.clone();
                    let auth_headers_3 = auth_headers_2.clone();
                    let peer_desc_func: Box<dyn FnMut(JsValue)> = Box::new(move |_: JsValue| {
                        let request =
                            XmlHttpRequest::new().expect("can't create new XmlHttpRequest");

                        request
                            .open("POST", &server_url_msg_2)
                            .unwrap_or_else(|err| {
                                info!("can't POST to server session url. {:?}", err)
                            });
                        if let Some(auth_bytes) = &auth_bytes_opt_3 {
                            let base64_encoded = base64::encode(auth_bytes);
                            request
                                .set_request_header("Authorization", &base64_encoded)
                                .expect("Failed to set request header");
                        }
                        // Consumer headers first, then naia's fingerprint header,
                        // which `stamp_protocol_id_header` already placed last.
                        for (key, value) in &auth_headers_3 {
                            request
                                .set_request_header(key, value)
                                .expect("Failed to set request header");
                        }

                        let request_2 = request.clone();
                        let peer_4 = peer_3.clone();
                        let addr_cell_4 = addr_cell_3.clone();
                        let addr_func_4 = addr_func_3.clone();
                        let id_sender_4 = id_sender_3.clone();
                        let request_func: Box<dyn FnMut(ProgressEvent)> = Box::new(
                            move |_: ProgressEvent| {
                                let status = request_2.status().unwrap();
                                warn!("naia: session POST status {}", status);
                                if status != 200 {
                                    // A rejection may carry a base64-encoded
                                    // message explaining itself
                                    // (naia-lib/naia#133). Before this the
                                    // non-200 branch did nothing at all and the
                                    // client waited forever.
                                    let body = request_2
                                        .response_text()
                                        .ok()
                                        .flatten()
                                        .unwrap_or_default();
                                    id_sender_4.send_error(status, body);
                                    return;
                                }
                                {
                                    let response_string =
                                        request_2.response_text().unwrap().unwrap();

                                    let session_response: JsSessionResponse =
                                        get_session_response(response_string.as_str());

                                    // Length only, never the value: the token
                                    // is an opaque secret after this hop.
                                    warn!(
                                        "naia: session id token len {}",
                                        session_response.id_token.len()
                                    );

                                    // send the id token to the client
                                    // info!("Sending id token to client: {:?}", auth_header);
                                    id_sender_4.send(session_response.id_token);

                                    let session_response_answer: SessionAnswer =
                                        session_response.answer.clone();

                                    let peer_5 = peer_4.clone();
                                    let addr_cell_5 = addr_cell_4.clone();
                                    let addr_func_5 = addr_func_4.clone();
                                    let remote_desc_func: Box<dyn FnMut(JsValue)> = Box::new(
                                        move |e: JsValue| {
                                            let candidate_str =
                                                session_response.candidate.candidate.as_str();

                                            addr_cell_5.receive_candidate(candidate_str);
                                            match addr_cell_5.get() {
                                                ServerAddr::Found(socket_addr) => {
                                                    addr_func_5
                                                        .as_ref()
                                                        .try_borrow_mut()
                                                        .expect("cannot borrow FindAddrFunc!")
                                                        .0(
                                                        socket_addr
                                                    );
                                                }
                                                _ => {
                                                    info!("error, not parsing address correctly?");
                                                }
                                            }

                                            let candidate_init_dict: RtcIceCandidateInit =
                                                RtcIceCandidateInit::new(candidate_str);
                                            candidate_init_dict.set_sdp_m_line_index(Some(
                                                session_response.candidate.sdp_m_line_index,
                                            ));
                                            candidate_init_dict.set_sdp_mid(Some(
                                                session_response.candidate.sdp_mid.as_str(),
                                            ));
                                            let candidate: RtcIceCandidate =
                                                RtcIceCandidate::new(&candidate_init_dict).unwrap();

                                            let peer_add_success_func: Box<dyn FnMut(JsValue)> =
                                                Box::new(move |_: JsValue| {
                                                    //Client add ice candidate
                                                    //success
                                                });
                                            let peer_add_success_callback =
                                                Closure::wrap(peer_add_success_func);
                                            let peer_add_failure_func: Box<dyn FnMut(JsValue)> =
                                                Box::new(move |_: JsValue| {
                                                    info!(
                                                    "Client error during 'addIceCandidate': {:?}",
                                                    e
                                                );
                                                });
                                            let peer_add_failure_callback =
                                                Closure::wrap(peer_add_failure_func);

                                            peer_5.add_ice_candidate_with_rtc_ice_candidate_and_success_callback_and_failure_callback(
                                                &candidate,
                                                peer_add_success_callback.as_ref().unchecked_ref(),
                                                peer_add_failure_callback.as_ref().unchecked_ref());
                                            peer_add_success_callback.forget();
                                            peer_add_failure_callback.forget();
                                        },
                                    );
                                    let remote_desc_callback = Closure::wrap(remote_desc_func);

                                    let rtc_session_desc_init_dict: RtcSessionDescriptionInit =
                                        RtcSessionDescriptionInit::new(RtcSdpType::Answer);

                                    rtc_session_desc_init_dict
                                        .set_sdp(session_response_answer.sdp.as_str());

                                    peer_4
                                        .set_remote_description(&rtc_session_desc_init_dict)
                                        .then(&remote_desc_callback);

                                    remote_desc_callback.forget();
                                }
                            },
                        );
                        let request_callback = Closure::wrap(request_func);
                        request.set_onload(Some(request_callback.as_ref().unchecked_ref()));
                        request_callback.forget();

                        // The offer is worthless without candidates, and naia's
                        // signaling is a single POST offer → answer with no
                        // trickle channel to recover later candidates — so a
                        // candidate-less early post is never allowed. But
                        // gating the send on gathering-complete with no bound
                        // tied to the connection deadline stalls connect for
                        // ~40 s when a single STUN path is slow (Roger 41874:
                        // icecandidateerror 701 held `complete` past the 30 s
                        // deadline despite usable srflx/host candidates from
                        // ~1 s in). Post on `complete`, or once
                        // ICE_GATHER_EARLY_POST_MS has passed with at least
                        // one candidate in hand — whichever comes first. The
                        // SDP is read at send time: gathering rewrites the
                        // local description in place, so a snapshot taken
                        // now would post the pre-gathering text.
                        let settled = Rc::new(RefCell::new(false));
                        let candidate_count = Rc::new(RefCell::new(0u32));
                        let gather_start_ms = Date::now();
                        let window = web_sys::window().expect("gathering gate needs a window");
                        let early_window = window.clone();
                        let timeout_cb = Closure::wrap(Box::new({
                            let settled = Rc::clone(&settled);
                            let id_sender = id_sender_3.clone();
                            move || {
                                if *settled.borrow() {
                                    return;
                                }
                                *settled.borrow_mut() = true;
                                // 504 is client-side: no server status exists
                                // for a gathering failure. The body rides
                                // base64 per naia-lib/naia#133 so the reason
                                // survives decode_reject_payload.
                                id_sender.send_error(
                                    504,
                                    base64::encode(format!(
                                        "ice gathering did not complete within {}ms: \
                                         session offer never posted",
                                        ICE_GATHERING_TIMEOUT_MS
                                    )),
                                );
                            }
                        })
                            as Box<dyn FnMut()>);
                        let timeout_handle = window
                            .set_timeout_with_callback_and_timeout_and_arguments_0(
                                timeout_cb.as_ref().unchecked_ref(),
                                ICE_GATHERING_TIMEOUT_MS,
                            )
                            .expect("gathering timeout must arm");
                        timeout_cb.forget();
                        let send_now = Rc::new({
                            let settled = Rc::clone(&settled);
                            let peer = peer_3.clone();
                            move || {
                                if *settled.borrow() {
                                    return;
                                }
                                *settled.borrow_mut() = true;
                                window.clear_timeout_with_handle(timeout_handle);
                                let offer_sdp = peer.local_description().unwrap().sdp();
                                // warn!: the served console shows warn-and-above
                                // only; this is one line per connection attempt.
                                warn!("naia: session POST send");
                                request
                                    .send_with_opt_str(Some(offer_sdp.as_str()))
                                    .unwrap_or_else(|err| {
                                        info!(
                                            "WebSys, can't sent request str. Original Error: {:?}",
                                            err
                                        )
                                    });
                            }
                        });
                        // Single decision point for all four wakeups (initial
                        // state, gathering-state changes, new candidates, and
                        // the early-post timer below): post on `complete`, or
                        // once the bounded wait has passed with candidates.
                        let maybe_post = Rc::new({
                            let peer = peer_3.clone();
                            let candidate_count = Rc::clone(&candidate_count);
                            let send_now = Rc::clone(&send_now);
                            move || {
                                let elapsed_ms = Date::now() - gather_start_ms;
                                if should_post_session_offer(
                                    peer.ice_gathering_state() == RtcIceGatheringState::Complete,
                                    *candidate_count.borrow(),
                                    elapsed_ms,
                                ) {
                                    send_now();
                                }
                            }
                        });
                        // New candidates feed the gate: a candidate arriving
                        // after the early-post timer fired still posts, since
                        // the wait has by then passed with candidates in hand.
                        let ice_cb = Closure::wrap(Box::new({
                            let candidate_count = Rc::clone(&candidate_count);
                            let maybe_post = Rc::clone(&maybe_post);
                            move |event: RtcPeerConnectionIceEvent| {
                                if event.candidate().is_some() {
                                    *candidate_count.borrow_mut() += 1;
                                }
                                maybe_post();
                            }
                        })
                            as Box<dyn FnMut(RtcPeerConnectionIceEvent)>);
                        peer_3.set_onicecandidate(Some(ice_cb.as_ref().unchecked_ref()));
                        ice_cb.forget();
                        let state_cb = Closure::wrap(Box::new({
                            let maybe_post = Rc::clone(&maybe_post);
                            move || {
                                maybe_post();
                            }
                        })
                            as Box<dyn FnMut()>);
                        peer_3
                            .set_onicegatheringstatechange(Some(state_cb.as_ref().unchecked_ref()));
                        state_cb.forget();
                        // Bounded wait: fires once the early-post bound has
                        // passed. Posts iff candidates arrived by then; with
                        // zero candidates this is a no-op and the 60 s
                        // backstop above still reports loudly.
                        let early_cb = Closure::wrap(Box::new({
                            let maybe_post = Rc::clone(&maybe_post);
                            move || {
                                maybe_post();
                            }
                        })
                            as Box<dyn FnMut()>);
                        early_window
                            .set_timeout_with_callback_and_timeout_and_arguments_0(
                                early_cb.as_ref().unchecked_ref(),
                                ICE_GATHER_EARLY_POST_MS,
                            )
                            .expect("gathering early-post timeout must arm");
                        early_cb.forget();
                        maybe_post();
                    });
                    let peer_desc_callback = Closure::wrap(peer_desc_func);

                    peer_2
                        .set_local_description(&session_description)
                        .then(&peer_desc_callback);
                    peer_desc_callback.forget();
                });
                let peer_offer_callback = Closure::wrap(peer_offer_func);

                let peer_error_func: Box<dyn FnMut(JsValue)> = Box::new(move |_: JsValue| {
                    info!("Client error during 'createOffer': e value here? TODO");
                });
                let peer_error_callback = Closure::wrap(peer_error_func);

                peer.create_offer().then(&peer_offer_callback);

                peer_offer_callback.forget();
                peer_error_callback.forget();

                // create message channel, get port
                let main_port = self.message_channel.port2();

                // setup RtcDataChannel onmessage handler
                let main_port_2 = main_port.clone();

                let channel_onmsg_func: Box<dyn FnMut(MessageEvent)> =
                    Box::new(move |evt: MessageEvent| {
                        main_port_2.post_message(&evt.data());
                    });
                let channel_onmsg_closure = Closure::wrap(channel_onmsg_func);

                channel.set_onmessage(Some(channel_onmsg_closure.as_ref().unchecked_ref()));
                channel_onmsg_closure.forget();

                // setup main_port onmessage handler
                let channel_2 = channel.clone();

                // Mirrors the miniquad bridge's "first send readyState": the
                // served console shows warn-and-above only, and a single line
                // per connection attempt names the channel state the first
                // outbound datagram actually met. Logged before the
                // open-check so a never-open channel still reports itself
                // instead of dropping silently.
                let first_send_logged = Rc::new(RefCell::new(false));
                let first_send_logged_2 = Rc::clone(&first_send_logged);

                let port_onmsg_func: Box<dyn FnMut(MessageEvent)> =
                    Box::new(move |evt: MessageEvent| {
                        if !*first_send_logged_2.borrow() {
                            *first_send_logged_2.borrow_mut() = true;
                            warn!("naia: first send readyState {:?}", channel_2.ready_state());
                        }
                        if let Ok(uarray) = evt.data().dyn_into::<js_sys::Uint8Array>() {
                            let mut body = vec![0; uarray.length() as usize];
                            uarray.copy_to(&mut body[..]);

                            if channel_2.ready_state() == RtcDataChannelState::Open {
                                channel_2
                                    .send_with_u8_array(&body.into_boxed_slice())
                                    .unwrap();
                            }
                        }
                    });
                let port_onmsg_closure = Closure::wrap(port_onmsg_func);

                main_port.set_onmessage(Some(port_onmsg_closure.as_ref().unchecked_ref()));
                port_onmsg_closure.forget();

                // Hand the live objects to the caller: the attempt's sender
                // retains this and closes both on shutdown, so a retried
                // attempt cannot leave a stale peer on the wire.
                WasmPeerCloser { peer, channel }
            }
            Err(err) => {
                panic!("error creating new RtcPeerConnection: {err:?}");
            }
        }
    }
}

#[derive(Clone)]
pub struct SessionAnswer {
    pub sdp: String,
}

pub struct SessionCandidate {
    pub candidate: String,
    pub sdp_m_line_index: u16,
    pub sdp_mid: String,
}

pub struct JsSessionResponse {
    pub id_token: IdentityToken,
    pub answer: SessionAnswer,
    pub candidate: SessionCandidate,
}

fn get_session_response(input: &str) -> JsSessionResponse {
    let json_obj: JsonValue = input.parse().unwrap();

    let sdp_opt: Option<&String> = json_obj["sdp"]["answer"]["sdp"].get();
    let sdp: String = sdp_opt.unwrap().clone();

    let candidate_opt: Option<&String> = json_obj["sdp"]["candidate"]["candidate"].get();
    let candidate: String = candidate_opt.unwrap().clone();

    let sdp_m_line_index_opt: Option<&f64> = json_obj["sdp"]["candidate"]["sdpMLineIndex"].get();
    let sdp_m_line_index: u16 = *(sdp_m_line_index_opt.unwrap()) as u16;

    let sdp_mid_opt: Option<&String> = json_obj["sdp"]["candidate"]["sdpMid"].get();
    let sdp_mid: String = sdp_mid_opt.unwrap().clone();

    let id_token_opt: Option<&String> = json_obj["id"].get();
    let id_token: IdentityToken =
        IdentityToken::from_signaling_string(id_token_opt.unwrap()).unwrap();

    JsSessionResponse {
        id_token,
        answer: SessionAnswer { sdp },
        candidate: SessionCandidate {
            candidate,
            sdp_m_line_index,
            sdp_mid,
        },
    }
}
