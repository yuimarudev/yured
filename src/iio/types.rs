use industrial_io as iio;

#[derive(Debug, Clone)]
pub struct AxisSet<T> {
  pub x: T,
  pub y: T,
  pub z: T,
}

#[derive(Debug, Clone)]
pub struct DiscoveredDevice {
  pub dev: iio::Device,
  pub accel: AxisSet<iio::Channel>,
  pub gyro: Option<AxisSet<iio::Channel>>,
  pub timestamp: Option<iio::Channel>,
}

impl<T> AxisSet<T> {
  pub fn as_array_ref(&self) -> [&T; 3] {
    [&self.x, &self.y, &self.z]
  }
}
