use log::{info, warn};

use super::{Session, Transport};
use crate::client::transport::*;
use crate::error::Error;

impl<T: Transport> super::XrpcClient<T> {
    /// Authenticate via `com.atproto.server.createSession`.
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

        let session: Session = serde_json::from_slice(&response.body)?;
        info!("authenticated as {} ({})", session.handle, session.did);
        self.session = Some(session);
        Ok(self.session.as_ref().unwrap())
    }

    /// Refresh the session using the stored refresh_jwt.
    pub(crate) async fn refresh_session(&mut self) -> Result<(), Error> {
        let refresh_jwt = self
            .session
            .as_ref()
            .map(|s| s.refresh_jwt.clone())
            .ok_or_else(|| Error::Auth("not logged in".into()))?;

        info!("access token expired, refreshing session");

        let response = self
            .transport
            .send(HttpRequest {
                method: HttpMethod::Post,
                url: format!("{}/xrpc/com.atproto.server.refreshSession", self.base_url),
                headers: vec![("Authorization".into(), format!("Bearer {}", refresh_jwt))],
                body: None,
            })
            .await?;

        if response.status != 200 {
            warn!("session refresh failed with HTTP {}", response.status);
            return Err(Error::Auth(format!(
                "session refresh failed (HTTP {}) — run `opake login` again",
                response.status
            )));
        }

        let new_session: Session = serde_json::from_slice(&response.body)?;
        info!("session refreshed for {}", new_session.handle);
        self.session = Some(new_session);
        self.session_refreshed = true;

        Ok(())
    }
}
