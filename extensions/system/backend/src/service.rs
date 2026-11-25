use std::time::Duration;

use serde_json::{json, Value};
use veronica_sdk::Context;
use veronica_system::metrics::SystemSnapshot;

pub(crate) const CPU: &str = "cpu";
pub(crate) const MEMORY: &str = "memory";
const POLL: Duration = Duration::from_secs(3);

pub(crate) fn cards(snapshot: &SystemSnapshot) -> [(&'static str, Value); 2] {
    let cpu = f64::from(snapshot.cpu.usage_percent).clamp(0.0, 100.0);
    let memory = snapshot.memory.used_percent().clamp(0.0, 100.0);
    [
        (CPU, json!({ "label": "CPU", "value": cpu / 100.0 })),
        (
            MEMORY,
            json!({ "label": "Memory", "value": memory / 100.0 }),
        ),
    ]
}

pub(crate) async fn publish(context: Context) {
    let mut last: [Option<Value>; 2] = [None, None];
    loop {
        if let Ok(snapshot) = crate::sampler::sample().await {
            let host = context.host();
            for ((surface, state), previous) in cards(&snapshot).into_iter().zip(&mut last) {
                if previous.as_ref() == Some(&state) {
                    continue;
                }
                if let Err(error) = host.surface_update(surface, state.clone()).await {
                    tracing::debug!("surface update refused: {}", error.message);
                }
                *previous = Some(state);
            }
        }
        tokio::time::sleep(POLL).await;
    }
}

#[cfg(test)]
mod tests {
    use veronica_system::metrics::SystemSnapshot;

    use super::cards;

    #[test]
    fn the_cards_carry_cpu_and_memory_as_fractions() {
        let mut snapshot = SystemSnapshot::default();
        snapshot.cpu.usage_percent = 25.0;
        snapshot.memory.total_bytes = 8;
        snapshot.memory.used_bytes = 6;
        let [cpu, memory] = cards(&snapshot);
        assert_eq!(cpu.1["value"], 0.25);
        assert_eq!(memory.1["value"], 0.75);
        assert_eq!(memory.1["label"], "Memory");
    }
}
