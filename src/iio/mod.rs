mod buffer;
mod channel;
mod discovery;
mod trigger;
mod types;

use self::buffer::BufferPoller;
use self::discovery::discover_best_device;
use self::trigger::{TriggerGuard, ensure_trigger_device};
use crate::error::{Error, Result};
use industrial_io as iio;

#[derive(Debug, Clone, Copy)]
pub struct ImuSample {
  pub accel_mps2: [f64; 3],
  pub gyro: Option<[f64; 3]>,
  pub dt_sec: f64,
}

pub struct IioPoller {
  poller: BufferPoller,
  rate_hz: u32,
  last_timestamp_ns: Option<i64>,
  trigger_guard: Option<TriggerGuard>,
}

impl IioPoller {
  pub fn open_best(rate_hz: u32) -> Result<Self> {
    let ctx = iio::Context::with_backend(iio::Backend::Local)?;

    match Self::open_best_in_context(&ctx, rate_hz, None) {
      Ok(poller) => Ok(poller),
      Err(Error::IioTriggerNotFound) => {
        let trigger_guard = ensure_trigger_device()?;
        let ctx = iio::Context::with_backend(iio::Backend::Local)?;

        match Self::open_best_in_context(&ctx, rate_hz, trigger_guard) {
          Ok(poller) => Ok(poller),
          Err(Error::IioTriggerNotFound) => Err(Error::invalid_state(
            "no iio trigger devices found after attempting auto-creation",
          )),
          Err(err) => Err(err),
        }
      }

      Err(err) => Err(err),
    }
  }

  fn open_best_in_context(
    ctx: &iio::Context,
    rate_hz: u32,
    trigger_guard: Option<TriggerGuard>,
  ) -> Result<Self> {
    let discovered = discover_best_device(ctx)?;
    let poller = BufferPoller::new(ctx, &discovered, rate_hz)?;

    Ok(Self {
      poller,
      rate_hz,
      last_timestamp_ns: None,
      trigger_guard,
    })
  }

  pub fn read_sample(&mut self) -> Result<ImuSample> {
    let _ = self.trigger_guard.as_ref();

    self
      .poller
      .read_sample(self.rate_hz, &mut self.last_timestamp_ns)
  }
}
