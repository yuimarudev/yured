use crate::error::{Error, Result};
use std::io;
use std::net::TcpStream;
use std::time::Duration;
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket};
use url::Url;

pub struct WsClient {
  url: Url,
  socket: Option<WebSocket<MaybeTlsStream<TcpStream>>>,
}

impl WsClient {
  pub fn new(url: Url) -> Self {
    Self { url, socket: None }
  }

  pub fn is_connected(&self) -> bool {
    self.socket.is_some()
  }

  pub fn poll_connect(&mut self) -> Result<()> {
    self.maybe_connect()
  }

  pub fn send_text(&mut self, text: String) -> Result<bool> {
    self.maybe_connect()?;

    let Some(mut socket) = self.socket.take() else {
      return Ok(false);
    };

    match socket.send(Message::Text(text)) {
      Ok(()) => {
        self.socket = Some(socket);

        Ok(true)
      }

      Err(err) => Err(Error::from(err)),
    }
  }

  pub fn poll_incoming(&mut self) -> Result<()> {
    let Some(mut socket) = self.socket.take() else {
      return Ok(());
    };

    loop {
      match socket.read() {
        Ok(message) => match message {
          Message::Ping(payload) => {
            socket.send(Message::Pong(payload)).map_err(Error::from)?;
          }

          Message::Close(frame) => {
            let _ = socket.close(frame);

            return Ok(());
          }

          _ => {}
        },

        Err(tungstenite::Error::Io(err))
          if matches!(
            err.kind(),
            io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut | io::ErrorKind::Interrupted
          ) =>
        {
          self.socket = Some(socket);

          return Ok(());
        }

        Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed) => {
          return Ok(());
        }

        Err(err) => {
          return Err(Error::from(err));
        }
      }
    }
  }

  fn maybe_connect(&mut self) -> Result<()> {
    if self.socket.is_some() {
      return Ok(());
    }

    match tungstenite::connect(self.url.clone()) {
      Ok((socket, _response)) => {
        let mut socket = socket;

        Self::configure_socket(&mut socket)?;
        self.socket = Some(socket);

        Ok(())
      }

      Err(err) => Err(Error::from(err)),
    }
  }

  fn configure_socket(socket: &mut WebSocket<MaybeTlsStream<TcpStream>>) -> Result<()> {
    let stream = socket.get_mut();
    let timeout = Duration::from_millis(10);
    let result = match stream {
      MaybeTlsStream::Plain(stream) => stream.set_read_timeout(Some(timeout)),
      MaybeTlsStream::Rustls(stream) => stream.get_mut().set_read_timeout(Some(timeout)),
      _ => Ok(()),
    };

    result
      .map_err(|err| Error::invalid_state(format!("websocket configure has been fucked: {err}")))
  }
}
