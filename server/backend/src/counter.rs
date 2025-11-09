use std::num::NonZeroU64;

#[derive(thiserror::Error, Debug)]
pub enum Error {
  #[error("max must be greater than 0, got {0}")]
  InvalidMaximum(u64),
}

#[derive(Clone, Copy)]
pub struct Max(NonZeroU64);

impl Max {
  pub fn new(max: u64) -> Result<Self, Error> {
    NonZeroU64::new(max)
      .map(Self)
      .ok_or(Error::InvalidMaximum(max))
  }

  pub fn get(self) -> u64 {
    self.0.get()
  }
}

#[derive(Clone, Copy)]
pub struct Count(u64);

impl Count {
  pub fn new(value: u64) -> Self {
    Self(value)
  }

  pub fn get(&self) -> u64 {
    self.0
  }
}

#[derive(Clone)]
pub struct Counter {
  count: Count,
  max: Max,
}

impl Counter {
  pub fn new(max: Max) -> Self {
    Self {
      count: Count(0),
      max,
    }
  }

  pub fn max(&self) -> Max {
    self.max
  }

  pub fn count(&self) -> Count {
    self.count
  }

  pub fn inc(self) -> Self {
    let new_count = self.count.0.saturating_add(1).min(self.max.0.into());
    Self {
      count: Count(new_count),
      max: self.max,
    }
  }
}

pub fn reset(counter: Counter) -> Counter {
  Counter::new(counter.max())
}

pub fn is_saturated(counter: &Counter) -> bool {
  counter.count().get() >= counter.max().get()
}

// could make this more efficient by moving it into the api, but i think this
// consumption looks pretty...
pub fn update_max(counter: Counter, new_max: Max) -> Counter {
  let mut new_counter = Counter::new(new_max);
  for _ in 0..counter.count().get() {
    new_counter = new_counter.inc();
  }
  new_counter
}
