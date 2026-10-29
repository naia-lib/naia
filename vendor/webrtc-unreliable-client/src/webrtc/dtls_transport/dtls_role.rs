use crate::webrtc::sdp::description::session::SessionDescription;
use crate::webrtc::sdp::util::ConnectionRole;

use std::fmt;

/// DtlsRole indicates the role of the DTLS transport.
#[derive(Debug, Copy, Clone, PartialEq, Default)]
pub(crate) enum DTLSRole {
    #[default]
    Unspecified = 0,

    /// DTLSRoleAuto defines the DTLS role is determined based on
    /// the resolved ICE role: the ICE controlled role acts as the DTLS
    /// client and the ICE controlling role acts as the DTLS server.
    Auto = 1,

    /// DTLSRoleClient defines the DTLS client role.
    Client = 2,

    /// DTLSRoleServer defines the DTLS server role.
    Server = 3,
}

/// The endpoint that is the offerer MUST use the setup attribute
/// value of setup:actpass and be prepared to receive a client_hello
/// before it receives the answer.
pub(crate) const DEFAULT_DTLS_ROLE_OFFER: DTLSRole = DTLSRole::Auto;

impl fmt::Display for DTLSRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            DTLSRole::Auto => write!(f, "auto"),
            DTLSRole::Client => write!(f, "client"),
            DTLSRole::Server => write!(f, "server"),
            _ => write!(f, "{}", crate::webrtc::UNSPECIFIED_STR),
        }
    }
}

/// Iterate a SessionDescription from a remote to determine if an explicit
/// role can been determined from it. The decision is made from the first role we we parse.
/// If no role can be found we return DTLSRoleAuto
impl From<&SessionDescription> for DTLSRole {
    fn from(session_description: &SessionDescription) -> Self {
        for media_section in &session_description.media_descriptions {
            for attribute in &media_section.attributes {
                if attribute.key == "setup" {
                    if let Some(value) = &attribute.value {
                        match value.as_str() {
                            "active" => return DTLSRole::Client,
                            "passive" => return DTLSRole::Server,
                            _ => return DTLSRole::Auto,
                        };
                    } else {
                        return DTLSRole::Auto;
                    }
                }
            }
        }

        DTLSRole::Auto
    }
}

impl DTLSRole {
    pub(crate) fn to_connection_role(self) -> ConnectionRole {
        match self {
            DTLSRole::Client => ConnectionRole::Active,
            DTLSRole::Server => ConnectionRole::Passive,
            DTLSRole::Auto => ConnectionRole::Actpass,
            _ => ConnectionRole::Unspecified,
        }
    }
}
