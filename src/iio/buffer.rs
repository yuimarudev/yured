use super::channel::{
  ChannelConfig, axis_config_with_sample_type, channel_sample_type, read_axis_scaled,
  read_first_sample_as_i64,
};
use super::trigger::{
  configure_sampling_frequency, disable_iio_buffer, is_device_busy_error, is_device_timeout_error,
  select_trigger, set_trigger,
};
use super::types::{AxisSet, DiscoveredDevice};
use crate::error::{Error, Result};
use industrial_io as iio;
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug)]
pub struct BufferPoller {
  buffer: iio::Buffer,
  accel: AxisSet<ChannelConfig>,
  gyro: Option<AxisSet<ChannelConfig>>,
  timestamp: Option<ChannelConfig>,
  sysfs_trigger: Option<iio::Device>,
  sysfs_trigger_period: Duration,
  sysfs_trigger_last_fire: Option<Instant>,
}

impl BufferPoller {
  pub fn new(ctx: &iio::Context, discovered: &DiscoveredDevice, rate_hz: u32) -> Result<Self> {
    if !discovered.dev.is_buffer_capable() {
      return Err(Error::invalid_state("device is not buffer capable"));
    }

    let accel = axis_config_with_sample_type(&discovered.accel)?;
    let gyro = discovered
      .gyro
      .as_ref()
      .map(axis_config_with_sample_type)
      .transpose()?;
    let timestamp = discovered
      .timestamp
      .as_ref()
      .filter(|chan| chan.is_scan_element())
      .map(|chan| -> Result<ChannelConfig> {
        Ok(ChannelConfig {
          chan: chan.clone(),
          scale: 1.0,
          offset: 0,
          sample_type: Some(channel_sample_type(chan)?),
        })
      })
      .transpose()?;

    let mut enable = Vec::new();

    enable.extend(accel.as_array_ref().iter().copied());

    if let Some(gyro) = gyro.as_ref() {
      enable.extend(gyro.as_array_ref().iter().copied());
    }

    if let Some(ts) = timestamp.as_ref() {
      enable.push(ts);
    }

    let scan_inputs: Vec<iio::Channel> = discovered
      .dev
      .channels()
      .filter(|chan| chan.is_scan_element() && chan.is_input())
      .collect();

    for chan in &scan_inputs {
      chan.disable();
    }

    for chan in &enable {
      chan.chan.enable();
    }

    let enabled_scan_inputs = scan_inputs.iter().filter(|chan| chan.is_enabled()).count();
    let triggers: Vec<iio::Device> = ctx.devices().filter(iio::Device::is_trigger).collect();

    if triggers.is_empty() {
      return Err(Error::IioTriggerNotFound);
    }

    let trigger = select_trigger(&triggers);

    configure_sampling_frequency(&discovered.dev, trigger.as_ref(), &enable, rate_hz)?;

    let trigger_name = set_trigger(&discovered.dev, trigger.as_ref())?;

    eprintln!("iio trigger: {trigger_name:?}");

    if enabled_scan_inputs == 0 {
      return Err(Error::invalid_state(
        "no scan element channels enabled; cannot create iio buffer",
      ));
    }

    let buffer = match discovered.dev.create_buffer(1, false) {
      Ok(buffer) => buffer,
      Err(err) if is_device_busy_error(&err) => {
        disable_iio_buffer(&discovered.dev)?;

        match discovered.dev.create_buffer(1, false) {
          Ok(buffer) => buffer,
          Err(err) if is_device_busy_error(&err) => return Err(err.into()),
          Err(err) => {
            let sample_size = discovered.dev.sample_size().ok();

            return Err(Error::invalid_state(format!(
              "failed to create iio buffer: {err} (device={:?} name={:?} trigger={:?} enabled_scan_inputs={enabled_scan_inputs} sample_size={sample_size:?})",
              discovered.dev.id(),
              discovered.dev.name(),
              trigger_name.as_ref(),
            )));
          }
        }
      }

      Err(err) => {
        let sample_size = discovered.dev.sample_size().ok();

        return Err(Error::invalid_state(format!(
          "failed to create iio buffer: {err} (device={:?} name={:?} trigger={:?} enabled_scan_inputs={enabled_scan_inputs} sample_size={sample_size:?})",
          discovered.dev.id(),
          discovered.dev.name(),
          trigger_name.as_ref(),
        )));
      }
    };

    let sysfs_trigger = trigger
      .as_ref()
      .filter(|trigger| trigger.has_attr("trigger_now"))
      .cloned();

    Ok(Self {
      buffer,
      accel,
      gyro,
      timestamp,
      sysfs_trigger,
      sysfs_trigger_period: Duration::from_nanos((1_000_000_000_u64 / u64::from(rate_hz)).max(1)),
      sysfs_trigger_last_fire: None,
    })
  }

  fn maybe_fire_sysfs_trigger(&mut self) -> Result<()> {
    let Some(trigger) = self.sysfs_trigger.as_ref() else {
      return Ok(());
    };

    if let Some(last_fire) = self.sysfs_trigger_last_fire {
      let elapsed = Instant::now().duration_since(last_fire);

      if let Some(sleep) = self.sysfs_trigger_period.checked_sub(elapsed)
        && !sleep.is_zero()
      {
        thread::sleep(sleep);
      }
    }

    trigger.attr_write_int("trigger_now", 1)?;
    self.sysfs_trigger_last_fire = Some(Instant::now());
    Ok(())
  }

  pub fn read_sample(
    &mut self,
    rate_hz: u32,
    last_timestamp_ns: &mut Option<i64>,
  ) -> Result<super::ImuSample> {
    self.maybe_fire_sysfs_trigger()?;

    match self.buffer.refill() {
      Ok(_len) => {}

      Err(err) if is_device_timeout_error(&err) => {
        let dev = self.buffer.device();

        return Err(Error::invalid_state(format!(
          "iio buffer refill timed out (device={:?} name={:?} sysfs_trigger_now={} rate_hz={rate_hz})",
          dev.id(),
          dev.name(),
          self.sysfs_trigger.is_some(),
        )));
      }

      Err(err) => return Err(err.into()),
    }

    let timestamp_ns = self
      .timestamp
      .as_ref()
      .map(|ts| read_first_sample_as_i64(&self.buffer, ts))
      .transpose()?;

    let dt_sec = match timestamp_ns {
      Some(ts) => {
        let dt_ns = last_timestamp_ns.and_then(|prev| ts.checked_sub(prev));
        *last_timestamp_ns = Some(ts);

        match dt_ns {
          Some(dt_ns) if dt_ns > 0 => {
            let dt_ns = u64::try_from(dt_ns).unwrap_or(0);

            if dt_ns == 0 {
              1.0 / f64::from(rate_hz)
            } else {
              Duration::from_nanos(dt_ns).as_secs_f64()
            }
          }

          _ => 1.0 / f64::from(rate_hz),
        }
      }

      None => 1.0 / f64::from(rate_hz),
    };

    let accel_mps2 = read_axis_scaled(&self.buffer, &self.accel)?;
    let gyro = self
      .gyro
      .as_ref()
      .map(|gyro| read_axis_scaled(&self.buffer, gyro))
      .transpose()?;

    Ok(super::ImuSample {
      accel_mps2,
      gyro,
      dt_sec,
    })
  }
}
