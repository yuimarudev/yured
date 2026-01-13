use crate::{
  error::{Error, Result},
  fusion::Algorithm,
};
use rand::seq::IndexedRandom as _;
use serde::Serialize;
use sys_info_extended::{linux_os_release, os_release, os_type};

const YURE_ID_LEN: usize = 11;
const YURE_ID_CHARSET: &[u8; 8] = b"YUREyure";

#[derive(Clone, Debug, Serialize)]
pub struct YureSample<'a> {
  #[serde(rename = "yureId")]
  pub yure_id: &'a str,
  #[serde(rename = "userAgent")]
  pub user_agent: &'a str,
  pub x: f64,
  pub y: f64,
  pub z: f64,
  pub t: f64,
}

pub struct StreamBatcher<'a> {
  batch_size: usize,
  buf: Vec<YureSample<'a>>,
}

impl<'a> StreamBatcher<'a> {
  pub fn new(batch_size: usize) -> Self {
    Self {
      batch_size,
      buf: Vec::with_capacity(batch_size),
    }
  }

  pub fn push_sample(&mut self, sample: YureSample<'a>) -> Result<Option<String>> {
    self.buf.push(sample);

    if self.buf.len() < self.batch_size {
      return Ok(None);
    }

    let json = serde_json::to_string(&self.buf).map_err(Error::from)?;

    self.buf.clear();

    Ok(Some(json))
  }
}

pub fn generate_yure_id() -> String {
  let mut rng = rand::rng();

  String::from_utf8(
    (0..YURE_ID_LEN)
      .map(|_| *YURE_ID_CHARSET.choose(&mut rng).unwrap())
      .collect::<Vec<u8>>(),
  )
  .unwrap()
}

pub fn generate_user_agent(algo: Algorithm) -> String {
  let app_name = env!("CARGO_PKG_NAME");
  let app_version = env!("CARGO_PKG_VERSION");
  let arch = std::env::consts::ARCH;
  let name = linux_os_release()
    .map(|r| r.name().to_string())
    .unwrap_or(os_type().unwrap_or("Unknown".into()));
  let release = os_release().unwrap_or("unknown".into());

  format!("{app_name} v{app_version}-{algo} on {name} {release} {arch}")
}
