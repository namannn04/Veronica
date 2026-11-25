use std::sync::{Mutex, OnceLock, TryLockError};

use veronica_system::metrics::SystemSnapshot;
use veronica_system::MetricsSampler;

fn shared() -> &'static Mutex<MetricsSampler> {
    static SAMPLER: OnceLock<Mutex<MetricsSampler>> = OnceLock::new();
    SAMPLER.get_or_init(|| Mutex::new(MetricsSampler::new()))
}

pub(crate) async fn sample() -> anyhow::Result<SystemSnapshot> {
    tokio::task::spawn_blocking(|| {
        let mut sampler = match shared().try_lock() {
            Ok(sampler) => sampler,
            Err(TryLockError::WouldBlock) => {
                anyhow::bail!("the previous reading is still in progress")
            }
            Err(TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
        };
        Ok(sampler.sample())
    })
    .await?
}
