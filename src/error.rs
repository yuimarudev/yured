use std::borrow::Cow;
use std::fmt;
use std::time::SystemTimeError;

#[derive(Debug)]
pub enum Error {
  InvalidState(Cow<'static, str>),
  Time(SystemTimeError),
  Url(url::ParseError),
  Json(serde_json::Error),
  Iio(Box<industrial_io::Error>),
  Ws(Box<tungstenite::Error>),
  SensorNotFound,
  IioTriggerNotFound,
}
pub type Result<T> = std::result::Result<T, Error>;

impl Error {
  pub fn invalid_state(message: impl Into<Cow<'static, str>>) -> Self {
    Self::InvalidState(message.into())
  }
}

impl fmt::Display for Error {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      Self::InvalidState(message) => write!(f, "invalid state: {message}"),
      Self::Time(err) => write!(f, "time error: {err}"),
      Self::Url(err) => write!(f, "url parse error: {err}"),
      Self::Json(err) => write!(f, "json error: {err}"),
      Self::Iio(err) => write!(f, "iio error: {err}"),
      Self::Ws(err) => write!(f, "websocket error: {err}"),
      Self::SensorNotFound => write!(f, "iio sensor not found"),
      Self::IioTriggerNotFound => write!(f, "iio trigger not found"),
    }
  }
}

impl std::error::Error for Error {
  fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
    match self {
      Self::InvalidState(_) | Self::SensorNotFound | Self::IioTriggerNotFound => None,
      Self::Time(err) => Some(err),
      Self::Url(err) => Some(err),
      Self::Json(err) => Some(err),
      Self::Iio(err) => Some(err),
      Self::Ws(err) => Some(err),
    }
  }
}

impl From<SystemTimeError> for Error {
  fn from(err: SystemTimeError) -> Self {
    Self::Time(err)
  }
}

impl From<url::ParseError> for Error {
  fn from(err: url::ParseError) -> Self {
    Self::Url(err)
  }
}

impl From<serde_json::Error> for Error {
  fn from(err: serde_json::Error) -> Self {
    Self::Json(err)
  }
}

impl From<industrial_io::Error> for Error {
  fn from(err: industrial_io::Error) -> Self {
    Self::Iio(Box::new(err))
  }
}

impl From<tungstenite::Error> for Error {
  fn from(err: tungstenite::Error) -> Self {
    Self::Ws(Box::new(err))
  }
}
