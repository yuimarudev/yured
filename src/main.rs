#![deny(clippy::all, clippy::pedantic)]
#![feature(duration_millis_float)]
mod error;
mod fusion;
mod iio;
mod ws;
mod yure;

use crate::fusion::Algorithm;
use crate::yure::generate_user_agent;
use clap::Parser;
use error::Result;
use fusion::FusionEngine;
use iio::IioPoller;
use ringbuffer::{AllocRingBuffer, RingBuffer};
use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use ws::WsClient;
use yure::{StreamBatcher, YureSample, generate_yure_id};

#[derive(Clone, Debug, Parser)]
#[command(name = "yured")]
pub struct Config {
  #[arg(
    long,
    short,
    default_value_t = 30,
    value_parser = clap::builder::RangedU64ValueParser::<usize>::new().range(1..),
  )]
  pub batch: usize,
  #[arg(
    long,
    short,
    default_value_t = 100,
    value_parser = clap::builder::RangedU64ValueParser::<u32>::new().range(1..),
  )]
  pub rate: u32,
  #[arg(long, short, value_enum, default_value_t = Algorithm::Madgwick)]
  pub algorithm: Algorithm,
  #[arg(long, short)]
  pub verbose: bool,
}

#[derive(Clone, Copy, Debug)]
struct MotionSample {
  accel_linear: [f64; 3],
  t_ms: f64,
}

struct SampleQueue {
  queue: Mutex<AllocRingBuffer<MotionSample>>,
  not_empty: Condvar,
}

fn main() -> Result<()> {
  let config = Config::parse();
  let rate_hz = config.rate;
  let mut poller = IioPoller::open_best(rate_hz).unwrap();
  let yure_id = generate_yure_id();
  let queue = Arc::new(SampleQueue::new(config.batch));
  let (tx, rx) = mpsc::sync_channel::<String>(config.batch);
  let sender_config = config.clone();
  let sender_queue = Arc::clone(&queue);
  let sender_user_agent = generate_user_agent(config.algorithm, config.rate);
  let sender_yure_id = yure_id.clone();
  let ws_url = "wss://unstable.kusaremkn.com/yure/".try_into().unwrap();

  thread::spawn(move || {
    sender_loop(
      &sender_config,
      &sender_yure_id,
      &sender_queue,
      &sender_user_agent,
      &tx,
    );
  });

  thread::spawn(move || {
    ws_loop(ws_url, &rx);
  });

  let mut fusion = FusionEngine::new(config.algorithm, rate_hz);

  eprintln!("yureId: {yure_id}");

  loop {
    let sample = poller.read_sample()?;
    let t_ms = SystemTime::now()
      .duration_since(UNIX_EPOCH)
      .unwrap()
      .as_millis_f64();
    let gravity = fusion.update(sample.accel_mps2, sample.gyro, sample.dt_sec);
    let accel_with_gravity = sample.accel_mps2;
    let accel_linear = [
      accel_with_gravity[0] - gravity[0],
      accel_with_gravity[1] - gravity[1],
      accel_with_gravity[2] - gravity[2],
    ];

    queue.push_drop_old(MotionSample { accel_linear, t_ms });
  }
}

fn sender_loop(
  config: &Config,
  yure_id: &str,
  queue: &Arc<SampleQueue>,
  user_agent: &str,
  tx: &mpsc::SyncSender<String>,
) {
  let mut batch = StreamBatcher::new(config.batch);

  loop {
    let motion = queue.pop_wait();
    let sample = YureSample {
      yure_id,
      user_agent,
      x: motion.accel_linear[0],
      y: motion.accel_linear[1],
      z: motion.accel_linear[2],
      t: motion.t_ms,
    };

    if config.verbose
      && let Ok(line) = serde_json::to_string(&sample)
    {
      println!("{line}");
    }

    match batch.push_sample(sample) {
      Ok(Some(json)) => {
        let _ = tx.try_send(json);
      }
      Ok(None) => {}
      Err(err) => {
        eprintln!("{err}");
      }
    }
  }
}

fn ws_loop(url: url::Url, rx: &mpsc::Receiver<String>) {
  let mut ws = WsClient::new(url);

  loop {
    if !ws.is_connected()
      && let Err(err) = ws.poll_connect()
    {
      eprintln!("{err}");
      thread::sleep(Duration::from_millis(200));

      continue;
    }

    match rx.recv_timeout(Duration::from_millis(10)) {
      Ok(json) => {
        if let Err(err) = ws.send_text(json) {
          eprintln!("{err}");
        }
      }
      Err(mpsc::RecvTimeoutError::Timeout) => {}
      Err(mpsc::RecvTimeoutError::Disconnected) => break,
    }

    if let Err(err) = ws.poll_incoming() {
      eprintln!("{err}");
    }
  }
}

impl SampleQueue {
  fn new(cap: usize) -> Self {
    Self {
      queue: Mutex::new(AllocRingBuffer::new(cap)),
      not_empty: Condvar::new(),
    }
  }

  fn push_drop_old(&self, item: MotionSample) {
    let mut guard = self.queue.lock().unwrap();
    let _ = guard.enqueue(item);

    self.not_empty.notify_one();
  }

  fn pop_wait(&self) -> MotionSample {
    let mut guard = self.queue.lock().unwrap();

    loop {
      if let Some(item) = guard.dequeue() {
        return item;
      }

      guard = self.not_empty.wait(guard).unwrap();
    }
  }
}
