use ahrs::Ahrs;
use clap::ValueEnum;
use nalgebra::{UnitQuaternion, Vector3};
use nalgebra_vqf::{UnitQuaternion as UnitQuaternionVqf, Vector3 as Vector3Vqf};
use num_traits::ToPrimitive;
use std::{fmt::Display, time::Duration};

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum Algorithm {
  Madgwick,
  Mahony,
  Vqf,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GravitySign {
  Unknown,
  Positive,
  Negative,
}

pub struct FusionEngine {
  inner: Box<dyn GravityEstimator>,
  gravity_sign: GravitySign,
}

trait GravityEstimator {
  fn update(&mut self, accel_mps2: [f64; 3], gyro_rad_s: [f64; 3], dt_sec: f64) -> [f64; 3];
}

impl GravitySign {
  fn factor(self) -> f64 {
    match self {
      Self::Unknown | Self::Positive => 1.0,
      Self::Negative => -1.0,
    }
  }

  fn from_dot(dot: f64) -> Self {
    if dot >= 0.0 {
      Self::Positive
    } else {
      Self::Negative
    }
  }
}

impl Display for Algorithm {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.write_str(match self {
      Algorithm::Madgwick => "madgwick",
      Algorithm::Mahony => "mahony",
      Algorithm::Vqf => "vqf",
    })
  }
}

impl FusionEngine {
  pub fn new(algorithm: Algorithm, rate_hz: u32) -> Self {
    let sample_period = 1.0 / f64::from(rate_hz);
    let inner: Box<dyn GravityEstimator> = match algorithm {
      Algorithm::Madgwick => Box::new(ahrs::Madgwick::new(sample_period, 0.1)),
      Algorithm::Mahony => Box::new(ahrs::Mahony::new(sample_period, 0.5, 0.0)),
      Algorithm::Vqf => {
        let period = Duration::from_secs_f64(sample_period);

        Box::new(vqf::Vqf::new(period, period, vqf::VqfParameters::default()))
      }
    };

    Self {
      inner,
      gravity_sign: GravitySign::Unknown,
    }
  }

  pub fn update(&mut self, accel_mps2: [f64; 3], gyro: Option<[f64; 3]>, dt_sec: f64) -> [f64; 3] {
    let gyro = gyro.unwrap_or([0.0; 3]);
    let g_body = self.inner.update(accel_mps2, gyro, dt_sec);

    maybe_calibrate_gravity_sign(&mut self.gravity_sign, accel_mps2, g_body);

    let factor = self.gravity_sign.factor();

    [g_body[0] * factor, g_body[1] * factor, g_body[2] * factor]
  }
}

impl GravityEstimator for ahrs::Madgwick<f64> {
  fn update(&mut self, accel_mps2: [f64; 3], gyro_rad_s: [f64; 3], dt_sec: f64) -> [f64; 3] {
    let dt = dt_sec.max(0.0);

    if dt > 0.0 {
      *self.sample_period_mut() = dt;
      let gyro = Vector3::new(gyro_rad_s[0], gyro_rad_s[1], gyro_rad_s[2]);
      let accel = Vector3::new(accel_mps2[0], accel_mps2[1], accel_mps2[2]);

      if self.update_imu(&gyro, &accel).is_err() {
        let _ = self.update_gyro(&gyro);
      }
    }

    gravity_from_orientation_f64(&self.quat)
  }
}

impl GravityEstimator for ahrs::Mahony<f64> {
  fn update(&mut self, accel_mps2: [f64; 3], gyro_rad_s: [f64; 3], dt_sec: f64) -> [f64; 3] {
    let dt = dt_sec.max(0.0);

    if dt > 0.0 {
      *self.sample_period_mut() = dt;
      let gyro = Vector3::new(gyro_rad_s[0], gyro_rad_s[1], gyro_rad_s[2]);
      let accel = Vector3::new(accel_mps2[0], accel_mps2[1], accel_mps2[2]);

      if self.update_imu(&gyro, &accel).is_err() {
        let _ = self.update_gyro(&gyro);
      }
    }

    gravity_from_orientation_f64(&self.quat)
  }
}

impl GravityEstimator for vqf::Vqf {
  fn update(&mut self, accel_mps2: [f64; 3], gyro_rad_s: [f64; 3], _dt_sec: f64) -> [f64; 3] {
    let Some(accel) = vec3_vqf_f32(accel_mps2) else {
      return gravity_from_orientation_vqf(&self.orientation());
    };
    let Some(gyro) = vec3_vqf_f32(gyro_rad_s) else {
      return gravity_from_orientation_vqf(&self.orientation());
    };

    self.update(gyro, accel);

    gravity_from_orientation_vqf(&self.orientation())
  }
}

fn maybe_calibrate_gravity_sign(sign: &mut GravitySign, accel_mps2: [f64; 3], g_body: [f64; 3]) {
  let accel_norm = (accel_mps2[0].powi(2) + accel_mps2[1].powi(2) + accel_mps2[2].powi(2)).sqrt();
  let g = 9.806_65;

  if !(0.5 * g..=1.5 * g).contains(&accel_norm) {
    return;
  }

  let dot = accel_mps2[0] * g_body[0] + accel_mps2[1] * g_body[1] + accel_mps2[2] * g_body[2];
  let gg = g * g;

  if dot.abs() < 0.2 * gg {
    return;
  }

  if *sign == GravitySign::Unknown {
    *sign = GravitySign::from_dot(dot);

    return;
  }

  let signed_dot = dot * sign.factor();

  if signed_dot < -0.8 * gg {
    *sign = match *sign {
      GravitySign::Positive => GravitySign::Negative,
      GravitySign::Negative => GravitySign::Positive,
      GravitySign::Unknown => GravitySign::from_dot(dot),
    };
  }
}

fn vec3_vqf_f32(v: [f64; 3]) -> Option<Vector3Vqf<f32>> {
  let x = v[0].to_f32()?;
  let y = v[1].to_f32()?;
  let z = v[2].to_f32()?;

  Some(Vector3Vqf::new(x, y, z))
}

fn gravity_from_orientation_f64(q_body_to_earth: &UnitQuaternion<f64>) -> [f64; 3] {
  let g_earth = Vector3::new(0.0, 0.0, 9.806_65);
  let g_body = q_body_to_earth.inverse_transform_vector(&g_earth);

  [g_body.x, g_body.y, g_body.z]
}

fn gravity_from_orientation_vqf(q_body_to_earth: &UnitQuaternionVqf<f32>) -> [f64; 3] {
  let g_earth = Vector3Vqf::new(0.0, 0.0, 9.806_65_f32);
  let g_body = q_body_to_earth.inverse_transform_vector(&g_earth);

  [
    f64::from(g_body.x),
    f64::from(g_body.y),
    f64::from(g_body.z),
  ]
}
