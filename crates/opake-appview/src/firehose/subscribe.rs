use futures_util::stream::SplitStream;
use futures_util::StreamExt;
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

use crate::error::{Error, Result};

type WsStream = SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>;

/// Build the Jetstream subscription URL with collection filters and optional cursor.
pub fn subscription_url(base_url: &str, cursor: Option<i64>) -> String {
    let mut url =
        format!("{base_url}?wantedCollections=app.opake.grant&wantedCollections=app.opake.keyring");
    if let Some(cursor_us) = cursor {
        url.push_str(&format!("&cursor={cursor_us}"));
    }
    url
}

/// Connect to the Jetstream WebSocket. Returns the read half of the stream.
pub async fn connect(url: &str) -> Result<WsStream> {
    log::info!("connecting to jetstream: {url}");
    let (ws, _response) = connect_async(url)
        .await
        .map_err(|e| Error::Firehose(format!("WebSocket connection failed: {e}")))?;
    let (_, read) = ws.split();
    Ok(read)
}

/// Read the next text message from the WebSocket stream.
/// Returns None if the stream is closed.
pub async fn next_message(stream: &mut WsStream) -> Result<Option<String>> {
    loop {
        match stream.next().await {
            Some(Ok(Message::Text(text))) => return Ok(Some(text.to_string())),
            Some(Ok(Message::Ping(_) | Message::Pong(_))) => continue,
            Some(Ok(Message::Close(_))) => return Ok(None),
            Some(Ok(_)) => continue,
            Some(Err(e)) => {
                return Err(Error::Firehose(format!("WebSocket error: {e}")));
            }
            None => return Ok(None),
        }
    }
}
