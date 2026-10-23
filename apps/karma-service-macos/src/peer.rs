//! Which connecting processes may send which requests.
//!
//! The kernel reports the uid and audit token of every connecting process.
//! Console requests are open to every local user because the service still
//! authenticates them with the administrator password. Agent requests must come
//! from the installed agent executable run by the user at the console; the
//! service then supplies the core's agent token itself, so the secret never
//! leaves this process. Installer requests must come from root.

use std::path::{Path, PathBuf};

use karma_ipc::{ClientKind, RequestEnvelope, ServiceErrorCode, ServiceRequest};
use zeroize::Zeroize;

/// The kernel-recorded facts about one connecting process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Peer {
    pub uid: u32,
    /// The image of exactly the connecting process instance, when it is
    /// still running it.
    pub executable: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct PeerPolicy {
    /// Canonical path of the installed agent executable, or `None` when it is
    /// not protected from replacement and no agent may connect.
    pub agent_executable: Option<PathBuf>,
    /// Installer requests must come from this uid: root for the LaunchDaemon,
    /// the developer for a `--console` run.
    pub installer_uid: u32,
}

impl PeerPolicy {
    /// Admits `request` from `peer`, supplying `agent_token` to an admitted
    /// agent request. `console_user` is the uid of the active graphical
    /// session; only that user's agent is accepted, as on Windows only the
    /// active console session runs one.
    pub fn admit(
        &self,
        request: &mut RequestEnvelope,
        peer: &Peer,
        console_user: Option<u32>,
        agent_token: &str,
    ) -> Result<(), ServiceErrorCode> {
        match request.client {
            ClientKind::Ui => Ok(()),
            ClientKind::Installer if peer.uid == self.installer_uid => Ok(()),
            ClientKind::Installer => Err(ServiceErrorCode::AuthenticationFailed),
            ClientKind::Agent
                if console_user == Some(peer.uid)
                    && peer
                        .executable
                        .as_deref()
                        .zip(self.agent_executable.as_deref())
                        .is_some_and(|(observed, expected)| same_file(observed, expected)) =>
            {
                supply_agent_token(&mut request.request, agent_token);
                Ok(())
            }
            ClientKind::Agent => Err(ServiceErrorCode::AgentUnauthorized),
        }
    }
}

fn same_file(observed: &Path, expected: &Path) -> bool {
    observed == expected
        || observed
            .canonicalize()
            .is_ok_and(|canonical| canonical == expected)
}

/// Replaces whatever token an admitted agent sent with the core's secret.
fn supply_agent_token(request: &mut ServiceRequest, token: &str) {
    match request {
        ServiceRequest::AgentHeartbeat { agent_token, .. }
        | ServiceRequest::AgentObservation { agent_token, .. }
        | ServiceRequest::AgentContextObservation { agent_token, .. }
        | ServiceRequest::SubmitEvidence { agent_token, .. }
        | ServiceRequest::GetAgentPolicy { agent_token }
        | ServiceRequest::ReportDisposition { agent_token, .. } => {
            agent_token.zeroize();
            agent_token.push_str(token);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AGENT: &str = "/Library/Karma/KarmaAgent.app/Contents/MacOS/karma-agent-macos";

    fn policy() -> PeerPolicy {
        PeerPolicy {
            agent_executable: Some(PathBuf::from(AGENT)),
            installer_uid: 0,
        }
    }

    fn agent_request() -> RequestEnvelope {
        RequestEnvelope::new(
            "request-1",
            "nonce-1",
            ClientKind::Agent,
            ServiceRequest::GetAgentPolicy {
                agent_token: "peer-supplied".into(),
            },
        )
    }

    fn peer(uid: u32, executable: &str) -> Peer {
        Peer {
            uid,
            executable: Some(PathBuf::from(executable)),
        }
    }

    fn token_of(request: &RequestEnvelope) -> &str {
        match &request.request {
            ServiceRequest::GetAgentPolicy { agent_token } => agent_token,
            _ => unreachable!(),
        }
    }

    #[test]
    fn the_console_users_installed_agent_receives_the_core_token() {
        let mut request = agent_request();
        policy()
            .admit(&mut request, &peer(501, AGENT), Some(501), "core-secret")
            .unwrap();
        assert_eq!(token_of(&request), "core-secret");
    }

    #[test]
    fn other_executables_and_other_users_cannot_act_as_the_agent() {
        for (peer, console) in [
            (peer(501, "/Users/child/karma-agent-macos"), Some(501)),
            (peer(502, AGENT), Some(501)),
            (peer(501, AGENT), None),
            (
                Peer {
                    uid: 501,
                    executable: None,
                },
                Some(501),
            ),
        ] {
            let mut request = agent_request();
            assert_eq!(
                policy().admit(&mut request, &peer, console, "core-secret"),
                Err(ServiceErrorCode::AgentUnauthorized)
            );
            assert_eq!(token_of(&request), "peer-supplied");
        }
    }

    #[test]
    fn no_agent_is_admitted_without_a_protected_agent_executable() {
        let mut request = agent_request();
        let unprotected = PeerPolicy {
            agent_executable: None,
            installer_uid: 0,
        };
        assert_eq!(
            unprotected.admit(&mut request, &peer(501, AGENT), Some(501), "core-secret"),
            Err(ServiceErrorCode::AgentUnauthorized)
        );
    }

    #[test]
    fn installer_requests_require_the_installer_uid() {
        let mut request = RequestEnvelope::new(
            "request-1",
            "nonce-1",
            ClientKind::Installer,
            ServiceRequest::RequestShutdown {
                password: "secret".into(),
            },
        );
        assert_eq!(
            policy().admit(&mut request, &peer(501, "/usr/bin/sudo"), Some(501), "t"),
            Err(ServiceErrorCode::AuthenticationFailed)
        );
        assert!(
            policy()
                .admit(&mut request, &peer(0, "/usr/bin/sudo"), Some(501), "t")
                .is_ok()
        );
    }

    #[test]
    fn console_requests_are_open_to_every_local_user() {
        let mut request = RequestEnvelope::new(
            "request-1",
            "nonce-1",
            ClientKind::Ui,
            ServiceRequest::GetBootstrap,
        );
        assert!(
            policy()
                .admit(
                    &mut request,
                    &peer(502, "/Applications/Karma.app"),
                    None,
                    "t"
                )
                .is_ok()
        );
    }
}
