use log::{info, warn};

use super::{Session, Transport};
use crate::client::session_refresh::{proactive_refresh, RefreshOutcome};
use crate::client::transport::*;
use crate::crypto::OsRng;
use crate::error::Error;

impl<T: Transport> super::XrpcClient<T> {
    /// Authenticate via `com.atproto.server.createSession` (legacy password flow).
    /// Returns a `Session::Legacy`.
    pub async fn login(&mut self, identifier: &str, password: &str) -> Result<&Session, Error> {
        info!("authenticating as {} against {}", identifier, self.base_url);

        let body = serde_json::json!({
            "identifier": identifier,
            "password": password,
        });

        let response = self
            .transport
            .send(HttpRequest {
                method: HttpMethod::Post,
                url: format!("{}/xrpc/com.atproto.server.createSession", self.base_url),
                headers: vec![("Content-Type".into(), "application/json".into())],
                body: Some(RequestBody::Json(body)),
            })
            .await?;

        if response.status != 200 {
            warn!("login failed with HTTP {}", response.status);
            return Err(Error::Auth(format!(
                "login failed (HTTP {})",
                response.status
            )));
        }

        let legacy: super::LegacySession = serde_json::from_slice(&response.body)?;
        info!("authenticated as {} ({})", legacy.handle, legacy.did);
        self.session = Some(Session::Legacy(legacy));
        Ok(self.session.as_ref().unwrap())
    }

    /// Refresh the session via the single `proactive_refresh` implementation.
    ///
    /// Uses threshold=i64::MAX so the refresh always fires (reactive use).
    /// The caller (`send_checked`) invokes this after the PDS returns
    /// `ExpiredToken` — there's no separate reactive implementation.
    pub(crate) async fn refresh_session(&mut self) -> Result<(), Error> {
        let session = self
            .session
            .take()
            .ok_or_else(|| Error::Auth("not logged in".into()))?;

        let now = crate::client::time::unix_now();
        let outcome = proactive_refresh(
            &self.transport,
            &session,
            &self.base_url,
            86400 * 365 * 100, // always refresh — we already know the token is expired
            now,
            &mut OsRng,
        )
        .await;

        match outcome {
            RefreshOutcome::Refreshed(new_session) => {
                info!("session refreshed for {}", new_session.handle());
                self.session = Some(*new_session);
                self.session_refreshed = true;
                Ok(())
            }
            RefreshOutcome::NotNeeded => {
                // Shouldn't happen with i64::MAX threshold, but handle gracefully.
                self.session = Some(session);
                Ok(())
            }
            RefreshOutcome::Failed(e) => {
                // Put the old session back so subsequent calls can still try.
                self.session = Some(session);
                Err(e)
            }
        }
    }

    /// Replace the current session (used after login and proactive refresh).
    pub fn set_session(&mut self, session: Session) {
        self.session = Some(session);
    }
}
