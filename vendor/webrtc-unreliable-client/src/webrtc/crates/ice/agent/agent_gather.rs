use super::*;
use crate::webrtc::ice::network_type::*;
use crate::webrtc::ice::util::*;

use crate::webrtc::util::{vnet::net::*, Conn};

use crate::webrtc::ice::candidate::candidate_base::CandidateBaseConfig;
use crate::webrtc::ice::candidate::candidate_host::CandidateHostConfig;
use crate::webrtc::ice::candidate::*;
use std::sync::Arc;
use waitgroup::WaitGroup;

pub(crate) struct GatherCandidatesInternalParams {
    pub(crate) candidate_types: Vec<CandidateType>,
    pub(crate) network_types: Vec<NetworkType>,
    pub(crate) mdns_mode: MulticastDnsMode,
    pub(crate) mdns_name: String,
    pub(crate) net: Arc<Net>,
    pub(crate) interface_filter: Arc<Option<InterfaceFilterFn>>,
    pub(crate) ext_ip_mapper: Arc<Option<ExternalIpMapper>>,
    pub(crate) agent_internal: Arc<AgentInternal>,
    pub(crate) gathering_state: Arc<AtomicU8>,
    pub(crate) chan_candidate_tx: ChanCandidateTx,
}

struct GatherCandidatesLocalParams {
    network_types: Vec<NetworkType>,
    mdns_mode: MulticastDnsMode,
    mdns_name: String,
    interface_filter: Arc<Option<InterfaceFilterFn>>,
    ext_ip_mapper: Arc<Option<ExternalIpMapper>>,
    net: Arc<Net>,
    agent_internal: Arc<AgentInternal>,
}

impl Agent {
    pub(crate) async fn gather_candidates_internal(params: GatherCandidatesInternalParams) {
        Self::set_gathering_state(
            &params.chan_candidate_tx,
            &params.gathering_state,
            GatheringState::Gathering,
        )
        .await;

        let wg = WaitGroup::new();

        for t in &params.candidate_types {
            if t == &CandidateType::Host {
                let local_params = GatherCandidatesLocalParams {
                    network_types: params.network_types.clone(),
                    mdns_mode: params.mdns_mode,
                    mdns_name: params.mdns_name.clone(),
                    interface_filter: Arc::clone(&params.interface_filter),
                    ext_ip_mapper: Arc::clone(&params.ext_ip_mapper),
                    net: Arc::clone(&params.net),
                    agent_internal: Arc::clone(&params.agent_internal),
                };

                let w = wg.worker();
                smol::spawn(async move {
                    let _d = w;

                    Self::gather_candidates_local(local_params).await;
                })
                .detach();
            }
        }

        // Block until all STUN and TURN URLs have been gathered (or timed out)
        wg.wait().await;

        Self::set_gathering_state(
            &params.chan_candidate_tx,
            &params.gathering_state,
            GatheringState::Complete,
        )
        .await;
    }

    async fn set_gathering_state(
        chan_candidate_tx: &ChanCandidateTx,
        gathering_state: &Arc<AtomicU8>,
        new_state: GatheringState,
    ) {
        if GatheringState::from(gathering_state.load(Ordering::SeqCst)) != new_state
            && new_state == GatheringState::Complete
        {
            let cand_tx = chan_candidate_tx.lock().await;
            if let Some(tx) = &*cand_tx {
                let _ = tx.send(None).await;
            }
        }

        gathering_state.store(new_state as u8, Ordering::SeqCst);
    }

    async fn gather_candidates_local(params: GatherCandidatesLocalParams) {
        let (
            network_types,
            mdns_mode,
            mdns_name,
            interface_filter,
            ext_ip_mapper,
            net,
            agent_internal,
        ) = (
            params.network_types,
            params.mdns_mode,
            params.mdns_name,
            params.interface_filter,
            params.ext_ip_mapper,
            params.net,
            params.agent_internal,
        );

        let ips = local_interfaces(&net, &interface_filter, &network_types).await;
        for ip in ips {
            let mut mapped_ip = ip;

            if mdns_mode != MulticastDnsMode::QueryAndGather && ext_ip_mapper.is_some() {
                if let Some(ext_ip_mapper2) = ext_ip_mapper.as_ref() {
                    if ext_ip_mapper2.candidate_type == CandidateType::Host {
                        if let Ok(mi) = ext_ip_mapper2.find_external_ip(&ip.to_string()) {
                            mapped_ip = mi;
                        } else {
                            log::warn!(
                                "[{}]: 1:1 NAT mapping is enabled but no external IP is found for {}",
                                agent_internal.get_name(),
                                ip
                            );
                        }
                    }
                }
            }

            let address = if mdns_mode == MulticastDnsMode::QueryAndGather {
                mdns_name.clone()
            } else {
                mapped_ip.to_string()
            };

            //TODO: for network in networks
            let network = UDP.to_owned();

            let conn: Arc<dyn Conn + Send + Sync> =
                match listen_udp_in_port_range(&net, SocketAddr::new(ip, 0)).await {
                    Ok(conn) => conn,
                    Err(err) => {
                        log::warn!(
                            "[{}]: could not listen {} {}: {}",
                            agent_internal.get_name(),
                            network,
                            ip,
                            err
                        );
                        continue;
                    }
                };

            let port = match conn.local_addr().await {
                Ok(addr) => addr.port(),
                Err(err) => {
                    log::warn!(
                        "[{}]: could not get local addr: {}",
                        agent_internal.get_name(),
                        err
                    );
                    continue;
                }
            };

            let host_config = CandidateHostConfig {
                base_config: CandidateBaseConfig {
                    network: network.clone(),
                    address,
                    port,
                    component: COMPONENT_RTP,
                    conn: Some(conn),
                    ..CandidateBaseConfig::default()
                },
            };

            let candidate: Arc<dyn Candidate + Send + Sync> =
                match host_config.new_candidate_host().await {
                    Ok(candidate) => {
                        if mdns_mode == MulticastDnsMode::QueryAndGather {
                            if let Err(err) = candidate.set_ip(&ip).await {
                                log::warn!(
                                    "[{}]: Failed to create host candidate: {} {} {}: {:?}",
                                    agent_internal.get_name(),
                                    network,
                                    mapped_ip,
                                    port,
                                    err
                                );
                                continue;
                            }
                        }
                        Arc::new(candidate)
                    }
                    Err(err) => {
                        log::warn!(
                            "[{}]: Failed to create host candidate: {} {} {}: {}",
                            agent_internal.get_name(),
                            network,
                            mapped_ip,
                            port,
                            err
                        );
                        continue;
                    }
                };

            {
                if let Err(err) = agent_internal.add_candidate(&candidate).await {
                    if let Err(close_err) = candidate.close().await {
                        log::warn!(
                            "[{}]: Failed to close candidate: {}",
                            agent_internal.get_name(),
                            close_err
                        );
                    }
                    log::warn!(
                        "[{}]: Failed to append to localCandidates and run onCandidateHdlr: {}",
                        agent_internal.get_name(),
                        err
                    );
                }
            }
        }
    }
}
