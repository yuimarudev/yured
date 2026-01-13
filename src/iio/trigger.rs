use super::channel::ChannelConfig;
use crate::error::{Error, Result};
use industrial_io as iio;
use nix::errno::Errno;
use std::fs;
use std::path::{Path, PathBuf};

const HRTIMER_TRIGGER_BASE: &str = "/sys/kernel/config/iio/triggers/hrtimer";
const DEFAULT_HRTIMER_TRIGGER: &str = "yured-hrtimer";

pub struct TriggerGuard {
  path: PathBuf,
}

impl Drop for TriggerGuard {
  fn drop(&mut self) {
    let _ = fs::remove_dir(&self.path);
  }
}

pub fn ensure_trigger_device() -> Result<Option<TriggerGuard>> {
  karen::escalate_if_needed().map_err(|err| {
    Error::invalid_state(format!(
      "failed to escalate privileges for trigger creation: {err}"
    ))
  })?;

  create_hrtimer_trigger(DEFAULT_HRTIMER_TRIGGER)
}

fn create_hrtimer_trigger(name: &str) -> Result<Option<TriggerGuard>> {
  let base = Path::new(HRTIMER_TRIGGER_BASE);

  match fs::metadata(base) {
    Ok(meta) => {
      if !meta.is_dir() {
        return Err(Error::invalid_state(
          "iio hrtimer configfs is not a directory",
        ));
      }
    }

    Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
      return Err(Error::invalid_state(
        "iio hrtimer configfs is not available",
      ));
    }

    Err(err) => {
      return Err(Error::invalid_state(format!(
        "failed to access iio hrtimer configfs: {err}"
      )));
    }
  }

  let path = base.join(name);

  match fs::create_dir(&path) {
    Ok(()) => Ok(Some(TriggerGuard { path })),
    Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => Ok(None),
    Err(err) => Err(Error::invalid_state(format!(
      "failed to create iio hrtimer trigger at {}: {err}",
      path.display()
    ))),
  }
}

pub fn is_device_busy_error(err: &iio::Error) -> bool {
  match err {
    iio::Error::Nix(errno) => *errno == Errno::EBUSY,
    iio::Error::Io(err) => err.raw_os_error() == Some(Errno::EBUSY as i32),
    _ => false,
  }
}

pub fn is_device_timeout_error(err: &iio::Error) -> bool {
  match err {
    iio::Error::Nix(errno) => *errno == Errno::ETIMEDOUT,
    iio::Error::Io(err) => err.raw_os_error() == Some(Errno::ETIMEDOUT as i32),
    _ => false,
  }
}

pub fn is_device_access_error(err: &iio::Error) -> bool {
  match err {
    iio::Error::Nix(errno) => *errno == Errno::EACCES || *errno == Errno::EPERM,
    iio::Error::Io(err) => match err.raw_os_error() {
      Some(code) => code == Errno::EACCES as i32 || code == Errno::EPERM as i32,
      None => false,
    },
    _ => false,
  }
}

pub fn disable_iio_buffer(dev: &iio::Device) -> Result<()> {
  if !dev.is_buffer_capable() {
    return Ok(());
  }

  let mut result = dev.attr_write_bool("buffer/enable", false);

  if let Err(ref err) = result
    && is_device_access_error(err)
  {
    karen::escalate_if_needed().map_err(|err| {
      Error::invalid_state(format!(
        "failed to escalate privileges for iio buffer control: {err}"
      ))
    })?;
    result = dev.attr_write_bool("buffer/enable", false);
  }

  result.map_err(Error::from)
}

pub fn configure_sampling_frequency(
  dev: &iio::Device,
  trigger: Option<&iio::Device>,
  chans: &[&ChannelConfig],
  rate_hz: u32,
) -> Result<()> {
  let rate = i64::from(rate_hz);

  if let Some(trigger) = trigger
    && trigger.has_attr("sampling_frequency")
  {
    let mut result = trigger.attr_write_int("sampling_frequency", rate);

    if let Err(ref err) = result
      && is_device_access_error(err)
    {
      karen::escalate_if_needed().map_err(|err| {
        Error::invalid_state(format!(
          "failed to escalate privileges for trigger access: {err}"
        ))
      })?;
      result = trigger.attr_write_int("sampling_frequency", rate);
    }

    if let Err(ref err) = result
      && is_device_busy_error(err)
    {
      disable_iio_buffer(dev)?;

      result = trigger.attr_write_int("sampling_frequency", rate);
    }

    if let Err(err) = result {
      return Err(err.into());
    }
  }

  if dev.has_attr("sampling_frequency") {
    let mut result = dev.attr_write_int("sampling_frequency", rate);
    if let Err(ref err) = result
      && is_device_access_error(err)
    {
      karen::escalate_if_needed().map_err(|err| {
        Error::invalid_state(format!(
          "failed to escalate privileges for device access: {err}"
        ))
      })?;
      result = dev.attr_write_int("sampling_frequency", rate);
    }

    if let Err(ref err) = result
      && is_device_busy_error(err)
    {
      disable_iio_buffer(dev)?;
      result = dev.attr_write_int("sampling_frequency", rate);
    }

    if let Err(err) = result {
      return Err(err.into());
    }
  }

  for chan in chans {
    if !chan.chan.has_attr("sampling_frequency") {
      continue;
    }

    let mut result = chan.chan.attr_write_int("sampling_frequency", rate);

    if let Err(ref err) = result
      && is_device_access_error(err)
    {
      karen::escalate_if_needed().map_err(|err| {
        Error::invalid_state(format!(
          "failed to escalate privileges for channel access: {err}"
        ))
      })?;
      result = chan.chan.attr_write_int("sampling_frequency", rate);
    }

    if let Err(ref err) = result
      && is_device_busy_error(err)
    {
      disable_iio_buffer(dev)?;
      result = chan.chan.attr_write_int("sampling_frequency", rate);
    }

    if let Err(err) = result {
      return Err(err.into());
    }
  }

  Ok(())
}

pub fn set_trigger(dev: &iio::Device, trigger: Option<&iio::Device>) -> Result<Option<String>> {
  let Some(trigger) = trigger else {
    return Ok(None);
  };

  let trigger_name = trigger.name();
  let mut result = dev.set_trigger(trigger);

  if let Err(ref err) = result
    && is_device_access_error(err)
  {
    karen::escalate_if_needed().map_err(|err| {
      Error::invalid_state(format!(
        "failed to escalate privileges for trigger selection: {err}"
      ))
    })?;
    result = dev.set_trigger(trigger);
  }

  if let Err(ref err) = result
    && is_device_busy_error(err)
  {
    disable_iio_buffer(dev)?;
    result = dev.set_trigger(trigger);
  }

  match result {
    Ok(()) => Ok(trigger_name),
    Err(err) if is_device_busy_error(&err) => Err(err.into()),
    Err(err) => Err(Error::invalid_state(format!(
      "failed to set trigger {trigger_name:?}: {err}"
    ))),
  }
}

pub fn select_trigger(triggers: &[iio::Device]) -> Option<iio::Device> {
  triggers.first().cloned()
}
