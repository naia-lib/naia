use crate::webrtc::dtls_transport::dtls_fingerprint::RTCDtlsFingerprint;
use crate::webrtc::error::{Error, Result};
use crate::webrtc::ice_transport::ice_candidate::RTCIceCandidate;
use crate::webrtc::ice_transport::ice_gatherer::RTCIceGatherer;
use crate::webrtc::ice_transport::ice_gathering_state::RTCIceGatheringState;
use crate::webrtc::ice_transport::ice_parameters::RTCIceParameters;

pub(crate) mod sdp_type;
pub(crate) mod session_description;

use crate::webrtc::ice::candidate::candidate_base::unmarshal_candidate;
use crate::webrtc::ice::candidate::Candidate;
use crate::webrtc::peer_connection::MEDIA_SECTION_APPLICATION;
use crate::webrtc::sdp::description::common::{Address, ConnectionInformation};
use crate::webrtc::sdp::description::media::{MediaDescription, MediaName, RangedPort};
use crate::webrtc::sdp::description::session::*;
use crate::webrtc::sdp::util::ConnectionRole;
use std::convert::From;
use std::sync::Arc;

pub(crate) async fn add_candidates_to_media_descriptions(
    candidates: &[RTCIceCandidate],
    mut m: MediaDescription,
    ice_gathering_state: RTCIceGatheringState,
) -> Result<MediaDescription> {
    let append_candidate_if_new = |c: &dyn Candidate, m: MediaDescription| -> MediaDescription {
        let marshaled = c.marshal();
        for a in &m.attributes {
            if let Some(value) = &a.value {
                if &marshaled == value {
                    return m;
                }
            }
        }

        m.with_value_attribute("candidate".to_owned(), marshaled)
    };

    for c in candidates {
        let candidate = c.to_ice().await?;

        candidate.set_component(1);
        m = append_candidate_if_new(&candidate, m);

        candidate.set_component(2);
        m = append_candidate_if_new(&candidate, m);
    }

    if ice_gathering_state != RTCIceGatheringState::Complete {
        return Ok(m);
    }
    for a in &m.attributes {
        if &a.key == "end-of-candidates" {
            return Ok(m);
        }
    }

    Ok(m.with_property_attribute("end-of-candidates".to_owned()))
}

pub(crate) struct AddDataMediaSectionParams {
    should_add_candidates: bool,
    mid_value: String,
    ice_params: RTCIceParameters,
    dtls_role: ConnectionRole,
    ice_gathering_state: RTCIceGatheringState,
}

pub(crate) async fn add_data_media_section(
    d: SessionDescription,
    dtls_fingerprints: &[RTCDtlsFingerprint],
    candidates: &[RTCIceCandidate],
    params: AddDataMediaSectionParams,
) -> Result<SessionDescription> {
    let mut media = MediaDescription {
        media_name: MediaName {
            media: MEDIA_SECTION_APPLICATION.to_owned(),
            port: RangedPort {
                value: 9,
                range: None,
            },
            protos: vec!["UDP".to_owned(), "DTLS".to_owned(), "SCTP".to_owned()],
            formats: vec!["webrtc-datachannel".to_owned()],
        },
        media_title: None,
        connection_information: Some(ConnectionInformation {
            network_type: "IN".to_owned(),
            address_type: "IP4".to_owned(),
            address: Some(Address {
                address: "0.0.0.0".to_owned(),
                ttl: None,
                range: None,
            }),
        }),
        bandwidth: vec![],
        encryption_key: None,
        attributes: vec![],
    }
    .with_value_attribute(
        ATTR_KEY_CONNECTION_SETUP.to_owned(),
        params.dtls_role.to_string(),
    )
    .with_value_attribute(ATTR_KEY_MID.to_owned(), params.mid_value)
    .with_property_attribute("sendrecv".to_owned())
    .with_property_attribute("sctp-port:5000".to_owned())
    .with_ice_credentials(
        params.ice_params.username_fragment,
        params.ice_params.password,
    );

    for f in dtls_fingerprints {
        media = media.with_fingerprint(f.algorithm.clone(), f.value.to_uppercase());
    }

    if params.should_add_candidates {
        media = add_candidates_to_media_descriptions(candidates, media, params.ice_gathering_state)
            .await?;
    }

    Ok(d.with_media(media))
}

pub(crate) async fn populate_local_candidates(
    session_description: Option<&session_description::RTCSessionDescription>,
    ice_gatherer: Option<&Arc<RTCIceGatherer>>,
    ice_gathering_state: RTCIceGatheringState,
) -> Option<session_description::RTCSessionDescription> {
    if session_description.is_none() || ice_gatherer.is_none() {
        return session_description.cloned();
    }

    if let (Some(sd), Some(ice)) = (session_description, ice_gatherer) {
        let candidates = match ice.get_local_candidates().await {
            Ok(candidates) => candidates,
            Err(_) => return Some(sd.clone()),
        };

        let mut parsed = match sd.unmarshal() {
            Ok(parsed) => parsed,
            Err(_) => return Some(sd.clone()),
        };

        if !parsed.media_descriptions.is_empty() {
            let mut m = parsed.media_descriptions.remove(0);
            m = match add_candidates_to_media_descriptions(&candidates, m, ice_gathering_state)
                .await
            {
                Ok(m) => m,
                Err(_) => return Some(sd.clone()),
            };
            parsed.media_descriptions.insert(0, m);
        }

        Some(session_description::RTCSessionDescription {
            sdp_type: sd.sdp_type,
            sdp: parsed.marshal(),
            parsed: Some(parsed),
        })
    } else {
        None
    }
}

#[derive(Default)]
pub(crate) struct MediaSection {
    pub(crate) id: String,
    pub(crate) data: bool,
}

pub(crate) struct PopulateSdpParams {
    pub(crate) is_icelite: bool,
    pub(crate) connection_role: ConnectionRole,
    pub(crate) ice_gathering_state: RTCIceGatheringState,
}

/// populate_sdp serializes a PeerConnections state into an SDP
pub(crate) async fn populate_sdp(
    mut d: SessionDescription,
    dtls_fingerprints: &[RTCDtlsFingerprint],
    candidates: &[RTCIceCandidate],
    ice_params: &RTCIceParameters,
    media_sections: &[MediaSection],
    params: PopulateSdpParams,
) -> Result<SessionDescription> {
    let media_dtls_fingerprints = vec![];

    let mut bundle_value = "BUNDLE".to_owned();
    let mut bundle_count = 0;
    let append_bundle = |mid_value: &str, value: &mut String, count: &mut i32| {
        *value = value.clone() + " " + mid_value;
        *count += 1;
    };

    for (i, m) in media_sections.iter().enumerate() {
        let should_add_candidates = i == 0;

        let should_add_id = if m.data {
            let params = AddDataMediaSectionParams {
                should_add_candidates,
                mid_value: m.id.clone(),
                ice_params: ice_params.clone(),
                dtls_role: params.connection_role,
                ice_gathering_state: params.ice_gathering_state,
            };
            d = add_data_media_section(d, &media_dtls_fingerprints, candidates, params).await?;
            true
        } else {
            false
        };

        if should_add_id {
            append_bundle(&m.id, &mut bundle_value, &mut bundle_count);
        }
    }

    for fingerprint in dtls_fingerprints {
        d = d.with_fingerprint(
            fingerprint.algorithm.clone(),
            fingerprint.value.to_uppercase(),
        );
    }

    if params.is_icelite {
        // RFC 5245 S15.3
        d = d.with_value_attribute(ATTR_KEY_ICELITE.to_owned(), ATTR_KEY_ICELITE.to_owned());
    }

    Ok(d.with_value_attribute(ATTR_KEY_GROUP.to_owned(), bundle_value))
}

pub(crate) fn get_mid_value(media: &MediaDescription) -> Option<&String> {
    for attr in &media.attributes {
        if attr.key == "mid" {
            return attr.value.as_ref();
        }
    }
    None
}

pub(crate) fn description_is_plan_b(
    desc: Option<&session_description::RTCSessionDescription>,
) -> Result<bool> {
    if let Some(desc) = desc {
        if let Some(parsed) = &desc.parsed {
            for media in &parsed.media_descriptions {
                if let Some(s) = get_mid_value(media) {
                    if ["audio", "video", "data"]
                        .iter()
                        .any(|kind| s.eq_ignore_ascii_case(kind))
                    {
                        return Ok(true);
                    }
                }
            }
        }
    }
    Ok(false)
}

pub(crate) fn extract_fingerprint(desc: &SessionDescription) -> Result<(String, String)> {
    let mut fingerprints = vec![];

    if let Some(fingerprint) = desc.attribute("fingerprint") {
        fingerprints.push(fingerprint.clone());
    }

    for m in &desc.media_descriptions {
        if let Some(fingerprint) = m.attribute("fingerprint").and_then(|o| o) {
            fingerprints.push(fingerprint.to_owned());
        }
    }

    if fingerprints.is_empty() {
        return Err(Error::ErrSessionDescriptionNoFingerprint);
    }

    for m in 1..fingerprints.len() {
        if fingerprints[m] != fingerprints[0] {
            return Err(Error::ErrSessionDescriptionConflictingFingerprints);
        }
    }

    let parts: Vec<&str> = fingerprints[0].split(' ').collect();
    if parts.len() != 2 {
        return Err(Error::ErrSessionDescriptionInvalidFingerprint);
    }

    Ok((parts[1].to_owned(), parts[0].to_owned()))
}

pub(crate) async fn extract_ice_details(
    desc: &SessionDescription,
) -> Result<(String, String, Vec<RTCIceCandidate>)> {
    let mut candidates = vec![];
    let mut remote_pwds = vec![];
    let mut remote_ufrags = vec![];

    if let Some(ufrag) = desc.attribute("ice-ufrag") {
        remote_ufrags.push(ufrag.clone());
    }
    if let Some(pwd) = desc.attribute("ice-pwd") {
        remote_pwds.push(pwd.clone());
    }

    for m in &desc.media_descriptions {
        if let Some(ufrag) = m.attribute("ice-ufrag").and_then(|o| o) {
            remote_ufrags.push(ufrag.to_owned());
        }
        if let Some(pwd) = m.attribute("ice-pwd").and_then(|o| o) {
            remote_pwds.push(pwd.to_owned());
        }

        for a in &m.attributes {
            if a.is_ice_candidate() {
                if let Some(value) = &a.value {
                    let c: Arc<dyn Candidate + Send + Sync> =
                        Arc::new(unmarshal_candidate(value).await?);
                    let candidate = RTCIceCandidate::from(&c);
                    candidates.push(candidate);
                }
            }
        }
    }

    if remote_ufrags.is_empty() {
        return Err(Error::ErrSessionDescriptionMissingIceUfrag);
    } else if remote_pwds.is_empty() {
        return Err(Error::ErrSessionDescriptionMissingIcePwd);
    }

    for m in 1..remote_ufrags.len() {
        if remote_ufrags[m] != remote_ufrags[0] {
            return Err(Error::ErrSessionDescriptionConflictingIceUfrag);
        }
    }

    for m in 1..remote_pwds.len() {
        if remote_pwds[m] != remote_pwds[0] {
            return Err(Error::ErrSessionDescriptionConflictingIcePwd);
        }
    }

    Ok((remote_ufrags[0].clone(), remote_pwds[0].clone(), candidates))
}

pub(crate) fn have_application_media_section(desc: &SessionDescription) -> bool {
    for m in &desc.media_descriptions {
        if m.media_name.media == MEDIA_SECTION_APPLICATION {
            return true;
        }
    }

    false
}

/// update_sdp_origin saves sdp.Origin in PeerConnection when creating 1st local SDP;
/// for subsequent calling, it updates Origin for SessionDescription from saved one
/// and increments session version by one.
/// <https://tools.ietf.org/html/draft-ietf-rtcweb-jsep-25#section-5.2.2>
pub(crate) fn update_sdp_origin(origin: &mut Origin, d: &mut SessionDescription) {
    //TODO: if atomic.CompareAndSwapUint64(&origin.SessionVersion, 0, d.Origin.SessionVersion)
    if origin.session_version == 0 {
        // store
        origin.session_version = d.origin.session_version;
        //atomic.StoreUint64(&origin.SessionID, d.Origin.SessionID)
        origin.session_id = d.origin.session_id;
    } else {
        // load
        /*for { // awaiting for saving session id
            d.Origin.SessionID = atomic.LoadUint64(&origin.SessionID)
            if d.Origin.SessionID != 0 {
                break
            }
        }*/
        d.origin.session_id = origin.session_id;

        //d.Origin.SessionVersion = atomic.AddUint64(&origin.SessionVersion, 1)
        origin.session_version += 1;
        d.origin.session_version += 1;
    }
}
