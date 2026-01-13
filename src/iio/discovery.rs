use super::types::{AxisSet, DiscoveredDevice};
use crate::error::{Error, Result};
use industrial_io as iio;

pub fn discover_best_device(ctx: &iio::Context) -> Result<DiscoveredDevice> {
  let mut best_accel_only: Option<DiscoveredDevice> = None;
  let mut best_with_gyro: Option<DiscoveredDevice> = None;
  let mut best_with_gyro_timestamp: Option<DiscoveredDevice> = None;
  let mut saw_accel_without_scan_elements = false;

  for dev in ctx.devices() {
    if dev.is_trigger() {
      continue;
    }

    let Some(accel) = find_axis_channels(&dev, &["accel", "in_accel"]) else {
      continue;
    };
    if !accel
      .as_array_ref()
      .iter()
      .all(|chan| chan.is_scan_element())
    {
      saw_accel_without_scan_elements = true;
      continue;
    }

    if !dev.is_buffer_capable() {
      continue;
    }

    let gyro = find_axis_channels(&dev, &["anglvel", "in_anglvel"]);
    let timestamp = dev.find_input_channel("timestamp");
    let has_gyro = gyro.is_some();
    let has_timestamp = timestamp.is_some();
    let candidate = DiscoveredDevice {
      dev,
      accel,
      gyro,
      timestamp,
    };

    match (has_gyro, has_timestamp) {
      (true, true) => {
        best_with_gyro_timestamp = Some(candidate);
        break;
      }
      (true, false) => {
        if best_with_gyro.is_none() {
          best_with_gyro = Some(candidate);
        }
      }
      (false, _) => {
        if best_accel_only.is_none() {
          best_accel_only = Some(candidate);
        }
      }
    }
  }

  match best_with_gyro_timestamp
    .or(best_with_gyro)
    .or(best_accel_only)
  {
    Some(device) => Ok(device),
    None if saw_accel_without_scan_elements => Err(Error::invalid_state(
      "accel device found, but scan-elements/buffer are not available",
    )),
    None => Err(Error::SensorNotFound),
  }
}

fn find_axis_channels(dev: &iio::Device, prefixes: &[&str]) -> Option<AxisSet<iio::Channel>> {
  let mut chans: [Option<iio::Channel>; 3] = [None, None, None];

  for chan in dev.channels() {
    if !chan.is_input() {
      continue;
    }

    let Some(id) = chan.id() else { continue };
    let Some(axis) = axis_from_id(&id, prefixes) else {
      continue;
    };

    let next = match chans[axis].take() {
      None => chan,
      Some(prev) if chan.is_scan_element() && !prev.is_scan_element() => chan,
      Some(prev) => prev,
    };
    chans[axis] = Some(next);
  }

  Some(AxisSet {
    x: chans[0].take()?,
    y: chans[1].take()?,
    z: chans[2].take()?,
  })
}

pub fn axis_from_id(id: &str, prefixes: &[&str]) -> Option<usize> {
  for prefix in prefixes {
    if let Some(suffix) = id.strip_prefix(prefix) {
      return match suffix {
        "_x" => Some(0),
        "_y" => Some(1),
        "_z" => Some(2),
        _ => None,
      };
    }
  }
  None
}
