// From https://github.com/kyren/webrtc-unreliable/blob/master/src/interval.rs

use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
    time::{Duration, Instant},
};

use async_io::Timer;
use futures_core::Stream;

/// Simple stream of `std::time::Instant` at a target rate.
///
/// If the stream is polled late, the next instant will target the duration after the call to
/// `poll_next` that generated the event, not after the previous timer deadline.  Thus, under load
/// or with artificial delays, the stream will just not generate as many events rather than trying
/// to generate more events to catch up.  The target rate is the *fastest* rate the stream will run,
/// it may run slower.
pub struct Interval {
    duration: Duration,
    timer: Timer,
}

impl Interval {
    /// Create a new `Interval` stream where events are `duration` apart.
    pub fn new(duration: Duration) -> Interval {
        Interval {
            duration,
            timer: Timer::after(duration),
        }
    }
}

impl Stream for Interval {
    type Item = Instant;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let Self { duration, timer } = &mut *self;
        match Pin::new(&mut *timer).poll(cx) {
            Poll::Ready(instant) => {
                timer.set_after(*duration);
                Poll::Ready(Some(instant))
            }
            Poll::Pending => Poll::Pending,
        }
    }
}
