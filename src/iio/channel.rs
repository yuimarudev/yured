use super::types::AxisSet;
use crate::error::{Error, Result};
use industrial_io as iio;
use std::any::TypeId;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum SampleType {
  I8,
  I16,
  I32,
  I64,
  U8,
  U16,
  U32,
  U64,
}

#[derive(Debug, Clone)]
pub struct ChannelConfig {
  pub chan: iio::Channel,
  pub scale: f64,
  pub offset: i32,
  pub sample_type: Option<SampleType>,
}

impl SampleType {
  fn from_type_id(type_id: TypeId) -> Option<Self> {
    if type_id == TypeId::of::<i8>() {
      Some(Self::I8)
    } else if type_id == TypeId::of::<i16>() {
      Some(Self::I16)
    } else if type_id == TypeId::of::<i32>() {
      Some(Self::I32)
    } else if type_id == TypeId::of::<i64>() {
      Some(Self::I64)
    } else if type_id == TypeId::of::<u8>() {
      Some(Self::U8)
    } else if type_id == TypeId::of::<u16>() {
      Some(Self::U16)
    } else if type_id == TypeId::of::<u32>() {
      Some(Self::U32)
    } else if type_id == TypeId::of::<u64>() {
      Some(Self::U64)
    } else {
      None
    }
  }
}

pub fn channel_scale(chan: &iio::Channel) -> Result<f64> {
  let dfmt = chan.data_format();

  if chan.is_scan_element() {
    let scale = dfmt.scale();

    if scale != 0.0 {
      return Ok(scale);
    }
  }

  if !chan.has_attr("scale") {
    return Err(Error::invalid_state("channel missing scale"));
  }

  Ok(chan.attr_read_float("scale")?)
}

fn channel_offset(chan: &iio::Channel) -> Result<i32> {
  if !chan.has_attr("offset") {
    return Ok(0);
  }

  let offset = chan.attr_read_int("offset")?;

  offset
    .try_into()
    .map_err(|_err| Error::invalid_state("channel offset does not fit into i32"))
}

pub fn channel_sample_type(chan: &iio::Channel) -> Result<SampleType> {
  let Some(type_id) = chan.type_of() else {
    return Err(Error::invalid_state("unsupported channel sample type"));
  };

  SampleType::from_type_id(type_id)
    .ok_or_else(|| Error::invalid_state("unsupported channel sample type"))
}

pub fn axis_config_with_sample_type(
  axis: &AxisSet<iio::Channel>,
) -> Result<AxisSet<ChannelConfig>> {
  Ok(AxisSet {
    x: ChannelConfig {
      scale: channel_scale(&axis.x)?,
      offset: channel_offset(&axis.x)?,
      sample_type: Some(channel_sample_type(&axis.x)?),
      chan: axis.x.clone(),
    },
    y: ChannelConfig {
      scale: channel_scale(&axis.y)?,
      offset: channel_offset(&axis.y)?,
      sample_type: Some(channel_sample_type(&axis.y)?),
      chan: axis.y.clone(),
    },
    z: ChannelConfig {
      scale: channel_scale(&axis.z)?,
      offset: channel_offset(&axis.z)?,
      sample_type: Some(channel_sample_type(&axis.z)?),
      chan: axis.z.clone(),
    },
  })
}

pub fn read_axis_scaled(buffer: &iio::Buffer, axis: &AxisSet<ChannelConfig>) -> Result<[f64; 3]> {
  let [x, y, z] = axis.as_array_ref().map(|ch| read_first_scaled(buffer, ch));
  Ok([x?, y?, z?])
}

fn read_first_scaled(buffer: &iio::Buffer, cfg: &ChannelConfig) -> Result<f64> {
  let raw = read_first_sample_as_i64(buffer, cfg)?;
  let raw = raw
    .try_into()
    .map_err(|_err| Error::invalid_state("sample does not fit into i32"))?;
  Ok(apply_scale_offset(raw, cfg.offset, cfg.scale))
}

pub fn read_first_sample_as_i64(buffer: &iio::Buffer, cfg: &ChannelConfig) -> Result<i64> {
  let sample_type = cfg
    .sample_type
    .ok_or_else(|| Error::invalid_state("missing sample type"))?;
  let chan = &cfg.chan;

  match sample_type {
    SampleType::I8 => read_first(buffer, chan, iio::Buffer::channel_iter::<i8>),
    SampleType::I16 => read_first(buffer, chan, iio::Buffer::channel_iter::<i16>),
    SampleType::I32 => read_first(buffer, chan, iio::Buffer::channel_iter::<i32>),
    SampleType::I64 => read_first(buffer, chan, iio::Buffer::channel_iter::<i64>),
    SampleType::U8 => read_first(buffer, chan, iio::Buffer::channel_iter::<u8>),
    SampleType::U16 => read_first(buffer, chan, iio::Buffer::channel_iter::<u16>),
    SampleType::U32 => read_first(buffer, chan, iio::Buffer::channel_iter::<u32>),
    SampleType::U64 => read_first(buffer, chan, iio::Buffer::channel_iter::<u64>),
  }
}

fn read_first<T>(
  buffer: &iio::Buffer,
  chan: &iio::Channel,
  iter: for<'a> fn(&'a iio::Buffer, &'a iio::Channel) -> iio::buffer::Iter<'a, T>,
) -> Result<i64>
where
  T: Copy + 'static,
  i64: TryFrom<T>,
{
  let value = iter(buffer, chan)
    .next()
    .copied()
    .ok_or_else(|| Error::invalid_state("missing sample"))?;
  let converted = chan.convert(value);

  i64::try_from(converted).map_err(|_err| Error::invalid_state("invalid sample value"))
}

pub fn apply_scale_offset(raw: i32, offset: i32, scale: f64) -> f64 {
  (f64::from(raw) + f64::from(offset)) * scale
}
