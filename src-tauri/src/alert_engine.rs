use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::models::{AlertRecord, Direction, Market, PriceSample, WatchItem};

const MINUTE_MS: i64 = 60_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TriggerState {
    pub trigger_price: f64,
    pub cooldown_until: i64,
}

#[derive(Debug, Default)]
pub struct AlertEngine {
    states: HashMap<String, TriggerState>,
}

pub struct Evaluation<'a> {
    pub now: i64,
    pub market: Market,
    pub symbol: &'a str,
    pub price: f64,
    pub samples: &'a [PriceSample],
    pub watch: &'a WatchItem,
    pub periods: &'a [u16],
    pub rolling_24h_change: Option<f64>,
}

impl AlertEngine {
    pub fn evaluate(&mut self, input: Evaluation<'_>) -> Vec<AlertRecord> {
        if !input.watch.alert_enabled || input.price <= 0.0 || !input.price.is_finite() {
            return Vec::new();
        }
        let mut alerts = Vec::new();

        for &period in input.periods {
            let baseline = find_baseline(input.samples, input.now - i64::from(period) * MINUTE_MS);
            for direction in [Direction::Up, Direction::Down] {
                let key = state_key(input.market, input.symbol, period, direction);
                let state = self.states.get(&key);
                if state.is_some_and(|state| input.now < state.cooldown_until) {
                    continue;
                }
                let change_percent = if let Some(state) = state {
                    (input.price - state.trigger_price) / state.trigger_price * 100.0
                } else if period == 1440 {
                    input
                        .rolling_24h_change
                        .filter(|change| change.is_finite())
                        .or_else(|| {
                            baseline
                                .map(|sample| (input.price - sample.price) / sample.price * 100.0)
                        })
                        .unwrap_or(f64::NAN)
                } else {
                    baseline
                        .map(|sample| (input.price - sample.price) / sample.price * 100.0)
                        .unwrap_or(f64::NAN)
                };
                if !change_percent.is_finite() {
                    continue;
                }
                let threshold = match direction {
                    Direction::Up => input.watch.rise_threshold,
                    Direction::Down => input.watch.fall_threshold,
                };
                let reached = match direction {
                    Direction::Up => change_percent >= threshold,
                    Direction::Down => change_percent <= -threshold,
                };
                if !reached {
                    continue;
                }

                self.states.insert(
                    key,
                    TriggerState {
                        trigger_price: input.price,
                        cooldown_until: input.now + 5 * MINUTE_MS,
                    },
                );
                alerts.push(AlertRecord {
                    id: Uuid::new_v4().to_string(),
                    market: input.market,
                    symbol: input.symbol.to_owned(),
                    period,
                    direction,
                    change_percent,
                    trigger_price: input.price,
                    trigger_time: input.now,
                    expires_at: input.now + 6 * 60 * MINUTE_MS,
                    dismissed: false,
                });
            }
        }
        alerts
    }

    pub fn reset_symbol(&mut self, market: Market, symbol: &str) {
        let prefix = format!("{}:{symbol}:", market_name(market));
        self.states.retain(|key, _| !key.starts_with(&prefix));
    }

    pub fn snapshot(&self) -> &HashMap<String, TriggerState> {
        &self.states
    }

    pub fn restore(&mut self, states: HashMap<String, TriggerState>) {
        self.states = states
            .into_iter()
            .filter(|(_, state)| state.trigger_price.is_finite() && state.trigger_price > 0.0)
            .collect();
    }
}

fn find_baseline(samples: &[PriceSample], target: i64) -> Option<&PriceSample> {
    samples
        .iter()
        .filter(|sample| sample.timestamp <= target)
        .max_by_key(|sample| sample.timestamp)
}

fn state_key(market: Market, symbol: &str, period: u16, direction: Direction) -> String {
    let direction = match direction {
        Direction::Up => "up",
        Direction::Down => "down",
    };
    format!("{}:{symbol}:{period}:{direction}", market_name(market))
}

fn market_name(market: Market) -> &'static str {
    match market {
        Market::Usdm => "usdm",
        Market::Coinm => "coinm",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn watch() -> WatchItem {
        WatchItem {
            market: Market::Usdm,
            symbol: "BTCUSDT".into(),
            alert_enabled: true,
            rise_threshold: 5.0,
            fall_threshold: 5.0,
        }
    }

    fn samples() -> Vec<PriceSample> {
        (0..=65)
            .map(|minute| PriceSample {
                timestamp: minute * MINUTE_MS,
                price: 100.0,
            })
            .collect()
    }

    #[test]
    fn triggers_multiple_periods() {
        let mut engine = AlertEngine::default();
        let samples = samples();
        let watch = watch();
        let alerts = engine.evaluate(Evaluation {
            now: 65 * MINUTE_MS,
            market: Market::Usdm,
            symbol: "BTCUSDT",
            price: 106.0,
            samples: &samples,
            watch: &watch,
            periods: &[5, 10, 60],
            rolling_24h_change: None,
        });
        assert_eq!(alerts.len(), 3);
    }

    #[test]
    fn uses_trigger_price_after_cooldown() {
        let mut engine = AlertEngine::default();
        let samples = samples();
        let watch = watch();
        let make = |now, price| Evaluation {
            now,
            market: Market::Usdm,
            symbol: "BTCUSDT",
            price,
            samples: &samples,
            watch: &watch,
            periods: &[5],
            rolling_24h_change: None,
        };
        assert_eq!(engine.evaluate(make(65 * MINUTE_MS, 106.0)).len(), 1);
        assert!(engine.evaluate(make(67 * MINUTE_MS, 112.0)).is_empty());
        assert_eq!(engine.evaluate(make(71 * MINUTE_MS, 112.0)).len(), 1);
    }

    #[test]
    fn opposite_direction_has_independent_cooldown() {
        let mut engine = AlertEngine::default();
        let samples = samples();
        let watch = watch();
        engine.evaluate(Evaluation {
            now: 65 * MINUTE_MS,
            market: Market::Usdm,
            symbol: "BTCUSDT",
            price: 106.0,
            samples: &samples,
            watch: &watch,
            periods: &[5],
            rolling_24h_change: None,
        });
        let down = engine.evaluate(Evaluation {
            now: 66 * MINUTE_MS,
            market: Market::Usdm,
            symbol: "BTCUSDT",
            price: 94.0,
            samples: &samples,
            watch: &watch,
            periods: &[5],
            rolling_24h_change: None,
        });
        assert_eq!(down.len(), 1);
        assert_eq!(down[0].direction, Direction::Down);
    }

    #[test]
    fn triggers_twenty_four_hour_period() {
        let mut engine = AlertEngine::default();
        let samples = (0..=1445)
            .map(|minute| PriceSample {
                timestamp: minute * MINUTE_MS,
                price: 100.0,
            })
            .collect::<Vec<_>>();
        let alerts = engine.evaluate(Evaluation {
            now: 1445 * MINUTE_MS,
            market: Market::Usdm,
            symbol: "BTCUSDT",
            price: 106.0,
            samples: &samples,
            watch: &watch(),
            periods: &[1440],
            rolling_24h_change: None,
        });
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].period, 1440);
    }

    #[test]
    fn rolling_twenty_four_hour_change_triggers_without_kline_history() {
        let mut watch = watch();
        watch.rise_threshold = 1.0;
        watch.fall_threshold = 1.0;
        let alerts = AlertEngine::default().evaluate(Evaluation {
            now: 1_000_000,
            market: Market::Usdm,
            symbol: "SOLUSDT",
            price: 101.0,
            samples: &[],
            watch: &watch,
            periods: &[1440],
            rolling_24h_change: Some(-3.0),
        });
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].direction, Direction::Down);
        assert_eq!(alerts[0].change_percent, -3.0);
    }

    #[test]
    fn rolling_twenty_four_hour_alert_reuses_trigger_price_after_cooldown() {
        let mut watch = watch();
        watch.rise_threshold = 1.0;
        watch.fall_threshold = 1.0;
        let mut engine = AlertEngine::default();
        let evaluate = |now, price, engine: &mut AlertEngine| {
            engine.evaluate(Evaluation {
                now,
                market: Market::Usdm,
                symbol: "SOLUSDT",
                price,
                samples: &[],
                watch: &watch,
                periods: &[1440],
                rolling_24h_change: Some(-3.0),
            })
        };
        assert_eq!(evaluate(0, 100.0, &mut engine).len(), 1);
        assert!(evaluate(6 * MINUTE_MS, 99.5, &mut engine).is_empty());
        assert_eq!(evaluate(7 * MINUTE_MS, 98.9, &mut engine).len(), 1);
    }
}
