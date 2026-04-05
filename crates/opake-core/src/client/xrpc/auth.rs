use log::{info, warn};

use super::{LegacySession, Session, Transport};
// dpop re-exports used transitively via XrpcClient methods
use crate::client::oauth_token;
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

        let legacy: LegacySession = serde_json::from_slice(&response.body)?;
        info!("authenticated as {} ({})", legacy.handle, legacy.did);
        self.session = Some(Session::Legacy(legacy));
        Ok(self.session.as_ref().unwrap())
    }

    /// Refresh the session — dispatches to legacy or OAuth refresh.
    pub(crate) async fn refresh_session(&mut self) -> Result<(), Error> {
        let session = self
            .session
            .take()
            .ok_or_else(|| Error::Auth("not logged in".into()))?;

        match session {
            Session::Legacy(s) => self.refresh_legacy(s).await,
            Session::OAuth(s) => self.refresh_oauth(s).await,
        }
    }

    async fn refresh_legacy(&mut self, session: LegacySession) -> Result<(), Error> {
        info!("access token expired, refreshing legacy session");

        let response = self
            .transport
            .send(HttpRequest {
                method: HttpMethod::Post,
                url: format!("{}/xrpc/com.atproto.server.refreshSession", self.base_url),
                headers: vec![(
                    "Authorization".into(),
                    format!("Bearer {}", session.refresh_jwt),
                )],
                body: None,
            })
            .await?;

        if response.status != 200 {
            warn!("session refresh failed with HTTP {}", response.status);
            // Put the old session back so the user can still try other things
            self.session = Some(Session::Legacy(session));
            return Err(Error::Auth(format!(
                "session refresh failed (HTTP {})",
                response.status
            )));
        }

        let new_legacy: LegacySession = serde_json::from_slice(&response.body)?;
        info!("session refreshed for {}", new_legacy.handle);
        self.session = Some(Session::Legacy(new_legacy));
        self.session_refreshed = true;

        Ok(())
    }

    async fn refresh_oauth(&mut self, mut session: super::OAuthSession) -> Result<(), Error> {
        info!("access token expired, refreshing OAuth session");

        let timestamp = super::super::time::unix_now();
        let result = oauth_token::refresh_token(
            &self.transport,
            &session.token_endpoint,
            &session.client_id,
            &session.refresh_token,
            &session.dpop_key,
            &mut session.dpop_nonce,
            timestamp,
            &mut OsRng,
        )
        .await;

        match result {
            Ok(token_response) => {
                session.apply_token_response(&token_response, timestamp);
                info!("OAuth session refreshed for {}", session.handle);
                self.session = Some(Session::OAuth(session));
                self.session_refreshed = true;
                Ok(())
            }
            Err(e) => {
                self.session = Some(Session::OAuth(session));
                Err(e)
            }
        }
    }

    /// Replace the current session (used after login and proactive refresh).
    pub fn set_session(&mut self, session: Session) {
        self.session = Some(session);
    }
}
