extern crate log;

use std::{cell::RefCell, net::SocketAddr, rc::Rc};

use js_sys::{Array, Object, Reflect};
use log::info;
use tinyjson::JsonValue;
use wasm_bindgen::{closure::Closure, JsCast, JsValue};
use web_sys::{
    ErrorEvent, MessageChannel, MessageEvent, ProgressEvent, RtcConfiguration, RtcDataChannel,
    RtcDataChannelInit, RtcDataChannelState, RtcDataChannelType, RtcIceCandidate,
    RtcIceCandidateInit, RtcPeerConnection, RtcSdpType, RtcSessionDescriptionInit, XmlHttpRequest,
};

use naia_socket_shared::{parse_server_url, SocketConfig};

use crate::ServerAddr;

use super::{addr_cell::AddrCell, data_port::DataPort};

// FindAddrFuncInner
pub struct FindAddrFuncInner(pub Box<dyn FnMut(SocketAddr)>);

// PeerConnection
pub struct DataChannel {
    server_session_url: String,
    message_channel: MessageChannel,
    addr_cell: AddrCell,
    find_addr_func: Rc<RefCell<FindAddrFuncInner>>,
}

impl DataChannel {
    pub fn new(config: &SocketConfig, server_session_url: &str) -> Self {
        let server_url = parse_server_url(server_session_url);

        Self {
            server_session_url: format!("{}{}", server_url, config.rtc_endpoint_path.clone()),
            message_channel: MessageChannel::new().expect("can't create message channel"),
            addr_cell: AddrCell::default(),
            find_addr_func: Rc::new(RefCell::new(FindAddrFuncInner(Box::new(move |_| {})))),
        }
    }

    pub fn addr_cell(&self) -> AddrCell {
        self.addr_cell.clone()
    }

    pub fn data_port(&self) -> DataPort {
        DataPort::new(self.message_channel.port1())
    }

    pub fn on_find_addr(&mut self, func: Box<dyn FnMut(SocketAddr)>) {
        self.find_addr_func
            .as_ref()
            .try_borrow_mut()
            .expect("cannot borrow FindAddrFunc!")
            .0 = func;
    }

    #[allow(unused_must_use)]
    pub fn start(&self) {
        // Set up Ice Servers
        let ice_server_config_urls = Array::new();
        ice_server_config_urls.push(&JsValue::from("stun:stun.l.google.com:19302"));

        let ice_server_config = Object::new();
        Reflect::set(
            &ice_server_config,
            &JsValue::from("urls"),
            &JsValue::from(&ice_server_config_urls),
        );

        let ice_server_config_list = Array::new();
        ice_server_config_list.push(&ice_server_config);

        // Set up RtcConfiguration
        let mut peer_config: RtcConfiguration = RtcConfiguration::new();
        peer_config.ice_servers(&ice_server_config_list);

        // Setup Peer Connection
        match RtcPeerConnection::new_with_configuration(&peer_config) {
            Ok(peer) => {
                let mut data_channel_config: RtcDataChannelInit = RtcDataChannelInit::new();
                data_channel_config.ordered(false);
                data_channel_config.max_retransmits(0);

                let channel: RtcDataChannel =
                    peer.create_data_channel_with_data_channel_dict("data", &data_channel_config);
                channel.set_binary_type(RtcDataChannelType::Arraybuffer);

                let onerror_func: Box<dyn FnMut(ErrorEvent)> = Box::new(move |e: ErrorEvent| {
                    info!("data channel error event: {:?}", e);
                });
                let onerror_callback = Closure::wrap(onerror_func);
                channel.set_onerror(Some(onerror_callback.as_ref().unchecked_ref()));
                onerror_callback.forget();

                let peer_2 = peer.clone();
                let addr_cell_2 = self.addr_cell.clone();
                let addr_func_2 = self.find_addr_func.clone();
                let server_url_msg = self.server_session_url.clone();
                let peer_offer_func: Box<dyn FnMut(JsValue)> = Box::new(move |e: JsValue| {
                    let session_description = e.into();
                    let peer_3 = peer_2.clone();
                    let addr_cell_3 = addr_cell_2.clone();
                    let addr_func_3 = addr_func_2.clone();
                    let server_url_msg_2 = server_url_msg.clone();
                    let peer_desc_func: Box<dyn FnMut(JsValue)> = Box::new(move |_: JsValue| {
                        let request =
                            XmlHttpRequest::new().expect("can't create new XmlHttpRequest");

                        request
                            .open("POST", &server_url_msg_2)
                            .unwrap_or_else(|err| {
                                info!("can't POST to server session url. {:?}", err)
                            });

                        let request_2 = request.clone();
                        let peer_4 = peer_3.clone();
                        let addr_cell_4 = addr_cell_3.clone();
                        let addr_func_4 = addr_func_3.clone();
                        let request_func: Box<dyn FnMut(ProgressEvent)> = Box::new(
                            move |_: ProgressEvent| {
                                if request_2.status().unwrap() == 200 {
                                    let response_string =
                                        request_2.response_text().unwrap().unwrap();

                                    let session_response: JsSessionResponse =
                                        get_session_response(response_string.as_str());

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

                                            let mut candidate_init_dict: RtcIceCandidateInit =
                                                RtcIceCandidateInit::new(candidate_str);
                                            candidate_init_dict.sdp_m_line_index(Some(
                                                session_response.candidate.sdp_m_line_index,
                                            ));
                                            candidate_init_dict.sdp_mid(Some(
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

                                    let mut rtc_session_desc_init_dict: RtcSessionDescriptionInit =
                                        RtcSessionDescriptionInit::new(RtcSdpType::Answer);

                                    rtc_session_desc_init_dict
                                        .sdp(session_response_answer.sdp.as_str());

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

                        request
                            .send_with_opt_str(Some(
                                peer_3.local_description().unwrap().sdp().as_str(),
                            ))
                            .unwrap_or_else(|err| {
                                info!("WebSys, can't sent request str. Original Error: {:?}", err)
                            });
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

                let port_onmsg_func: Box<dyn FnMut(MessageEvent)> =
                    Box::new(move |evt: MessageEvent| {
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
            }
            Err(err) => {
                info!("Error creating new RtcPeerConnection. Error: {:?}", err);
                panic!("");
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
    pub answer: SessionAnswer,
    pub candidate: SessionCandidate,
}

fn get_session_response(input: &str) -> JsSessionResponse {
    let json_obj: JsonValue = input.parse().unwrap();

    let sdp_opt: Option<&String> = json_obj["answer"]["sdp"].get();
    let sdp: String = sdp_opt.unwrap().clone();

    let candidate_opt: Option<&String> = json_obj["candidate"]["candidate"].get();
    let candidate: String = candidate_opt.unwrap().clone();

    let sdp_m_line_index_opt: Option<&f64> = json_obj["candidate"]["sdpMLineIndex"].get();
    let sdp_m_line_index: u16 = *(sdp_m_line_index_opt.unwrap()) as u16;

    let sdp_mid_opt: Option<&String> = json_obj["candidate"]["sdpMid"].get();
    let sdp_mid: String = sdp_mid_opt.unwrap().clone();

    JsSessionResponse {
        answer: SessionAnswer { sdp },
        candidate: SessionCandidate {
            candidate,
            sdp_m_line_index,
            sdp_mid,
        },
    }
}
