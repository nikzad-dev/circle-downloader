use super::http::CancelToken;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::{
    sync::Mutex,
    time::{Duration, Instant},
};

/// FIFO admission in small quanta, shared by every segment and worker.
/// No initial burst; cancellation drops the wait and releases its place.
pub struct RateLimiter {
    rate: AtomicU64,
    next: Mutex<Instant>,
}
impl RateLimiter {
    pub fn new(rate: u64) -> Self {
        Self {
            rate: AtomicU64::new(rate),
            next: Mutex::new(Instant::now()),
        }
    }
    pub fn set_rate(&self, rate: u64) {
        self.rate.store(rate, Ordering::Relaxed);
    }
    pub async fn consume(&self, mut bytes: usize, cancel: &CancelToken) -> bool {
        while bytes > 0 {
            let rate = self.rate.load(Ordering::Relaxed);
            if rate == 0 {
                return !cancel.is_cancelled();
            }
            let amount = bytes.min((rate / 10).clamp(1, 16_384) as usize);
            let mut next = tokio::select! { biased; _=cancel.cancelled()=>return false, slot=self.next.lock()=>slot };
            // Rate changes apply at the next quantum; idle time never accrues a burst.
            let start = (*next).max(Instant::now());
            let deadline = start + Duration::from_secs_f64(amount as f64 / rate as f64);
            tokio::select! { biased; _=cancel.cancelled()=>return false, _=tokio::time::sleep_until(deadline)=>{} }
            *next = deadline;
            drop(next);
            bytes -= amount;
        }
        true
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    #[tokio::test(start_paused = true)]
    async fn aggregate_rate_and_fifo_fairness() {
        let limit = Arc::new(RateLimiter::new(1000));
        let started = Instant::now();
        let mut jobs = vec![];
        for _ in 0..4 {
            let l = limit.clone();
            jobs.push(tokio::spawn(async move {
                let (_sender, c) = super::super::http::cancel_pair();
                assert!(l.consume(500, &c).await);
                Instant::now()
            }));
        }
        let mut finished = vec![];
        for j in jobs {
            finished.push(j.await.unwrap());
        }
        assert!(started.elapsed() >= Duration::from_secs(2));
        let min = finished.iter().min().unwrap();
        let max = finished.iter().max().unwrap();
        assert!(*max - *min <= Duration::from_millis(400));
    }
    #[tokio::test]
    async fn cancellation_releases_wait_promptly() {
        let l = Arc::new(RateLimiter::new(1));
        let (tx, c) = super::super::http::cancel_pair();
        let child = l.clone();
        let job = tokio::spawn(async move { child.consume(100, &c).await });
        tx.send(true).unwrap();
        assert!(
            !tokio::time::timeout(Duration::from_millis(100), job)
                .await
                .unwrap()
                .unwrap()
        );
        l.set_rate(0);
        let (_sender, c) = super::super::http::cancel_pair();
        assert!(l.consume(100, &c).await);
    }
}
