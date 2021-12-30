use crate::portfolio::position::EquityPoint;
use crate::statistic::algorithm::welford_online;
use crate::statistic::dispersion::Range;
use chrono::{DateTime, Duration, Utc};

/// [`Drawdown`] is the peak-to-trough decline of the Portfolio, or investment, during a specific
/// period. Drawdown is a measure of downside volatility.
///
/// See documentation: <https://www.investopedia.com/terms/d/drawdown.asp>
#[derive(Debug, Copy, Clone, PartialOrd, PartialEq)]
pub struct Drawdown {
    pub equity_range: Range,
    pub drawdown: f64,
    pub start_timestamp: DateTime<Utc>,
    pub duration: Duration,
}

impl Default for Drawdown {
    fn default() -> Self {
        Self {
            equity_range: Default::default(),
            drawdown: 0.0,
            start_timestamp: Utc::now(),
            duration: Duration::zero(),
        }
    }
}

impl Drawdown {
    /// Initialises a new [`Drawdown`] using the starting equity as the first peak.
    pub fn init(starting_equity: f64) -> Self {
        Self {
            equity_range: Range {
                activated: true,
                high: starting_equity,
                low: starting_equity,
            },
            drawdown: 0.0,
            start_timestamp: Utc::now(),
            duration: Duration::zero(),
        }
    }

    /// Updates the [`Drawdown`] using the latest input [`EquityPoint`] of the Portfolio. If the drawdown
    /// period has ended (investment recovers from a trough back above the previous peak), the
    /// function return Some(Drawdown), else None is returned.
    pub fn update(&mut self, current: &EquityPoint) -> Option<Drawdown> {
        match (
            self.is_waiting_for_peak(),
            current.equity > self.equity_range.high,
        ) {
            // A) No current drawdown - waiting for next equity peak (waiting for B)
            (true, true) => {
                self.equity_range.high = current.equity;
                None
            }

            // B) Start of new drawdown - previous equity point set peak & current equity lower
            (true, false) => {
                self.start_timestamp = current.timestamp;
                self.equity_range.low = current.equity;
                self.drawdown = self.calculate();
                None
            }

            // C) Continuation of drawdown - equity lower than most recent peak
            (false, false) => {
                self.duration = current
                    .timestamp
                    .signed_duration_since(self.start_timestamp);
                self.equity_range.update(current.equity);
                self.drawdown = self.calculate(); // I don't need to calculate this now if I don't want
                None
            }

            // D) End of drawdown - equity has reached new peak (enters A)
            (false, true) => {
                // Clone Drawdown from previous iteration to return
                let finished_drawdown = Drawdown {
                    equity_range: self.equity_range.clone(),
                    drawdown: self.drawdown,
                    start_timestamp: self.start_timestamp,
                    duration: self.duration,
                };

                // Clean up - start_timestamp overwritten next drawdown start
                self.drawdown = 0.0; // ie/ waiting for peak = true
                self.duration = Duration::zero();

                // Set new equity peak in preparation for next iteration
                self.equity_range.high = current.equity;

                Some(finished_drawdown)
            }
        }
    }

    /// Determines if a [`Drawdown`] is waiting for the next equity peak. This is true if the new
    /// [`EquityPoint`] is higher than the previous peak.
    fn is_waiting_for_peak(&self) -> bool {
        self.drawdown == 0.0
    }

    /// Calculates the value of the [`Drawdown`] in the specific period. Uses the formula:
    /// [`Drawdown`] = (range_low - range_high) / range_high
    fn calculate(&self) -> f64 {
        // range_low - range_high / range_high
        (-self.equity_range.calculate()) / self.equity_range.high
    }
}

/// [`MaxDrawdown`] is the largest
/// peak-to-trough decline of the Portfolio, or investment. Max Drawdown is a measure of downside
/// risk, with large values indicating down movements could be volatile.
///
/// See documentation: <https://www.investopedia.com/terms/m/maximum-drawdown-mdd.asp>
#[derive(Debug, Copy, Clone, PartialOrd, PartialEq)]
pub struct MaxDrawdown {
    pub drawdown: Drawdown,
}

impl MaxDrawdown {
    /// Initialises a new [`MaxDrawdown`] using the [`Drawdown`] default value.
    pub fn init() -> Self {
        Self {
            drawdown: Drawdown::default(),
        }
    }

    /// Updates the [`MaxDrawdown`] using the latest input [`Drawdown`] of the Portfolio. If the input
    /// drawdown is larger than the current [`MaxDrawdown`], it supersedes it.
    pub fn update(&mut self, next_drawdown: &Drawdown) {
        if next_drawdown.drawdown.abs() > self.drawdown.drawdown.abs() {
            self.drawdown = next_drawdown.clone();
        }
    }
}

/// [`AvgDrawdown`] contains the average drawdown value and duration from a collection of [`Drawdown`]s
/// within a specific period.
#[derive(Debug, Copy, Clone, PartialOrd, PartialEq)]
pub struct AvgDrawdown {
    pub count: u64,
    pub mean_drawdown: f64,
    pub mean_duration: Duration,
    mean_duration_milliseconds: i64,
}

impl Default for AvgDrawdown {
    fn default() -> Self {
        Self {
            count: 0,
            mean_drawdown: 0.0,
            mean_duration_milliseconds: 0,
            mean_duration: Duration::zero(),
        }
    }
}

impl AvgDrawdown {
    /// Initialises a new [`AvgDrawdown`] using the default method, providing zero values for all
    /// fields.
    pub fn init() -> Self {
        Self::default()
    }

    /// Updates the [`AvgDrawdown`] using the latest input [`Drawdown`] of the Portfolio.
    pub fn update(&mut self, drawdown: &Drawdown) {
        self.count += 1;

        self.mean_drawdown = welford_online::calculate_mean(
            self.mean_drawdown,
            drawdown.drawdown,
            self.count as f64,
        );

        self.mean_duration_milliseconds = welford_online::calculate_mean(
            self.mean_duration_milliseconds,
            drawdown.duration.num_milliseconds(),
            self.count as i64,
        );

        self.mean_duration = Duration::milliseconds(self.mean_duration_milliseconds);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portfolio::position::EquityPoint;
    use std::ops::Add;

    #[test]
    fn drawdown_update() {
        struct TestCase {
            input_equity: EquityPoint,
            expected_drawdown: Drawdown,
        }
        let base_timestamp = Utc::now();
        let starting_equity = 100.0;

        let mut drawdown = Drawdown {
            equity_range: Range {
                activated: true,
                high: starting_equity,
                low: starting_equity,
            },
            drawdown: 0.0,
            start_timestamp: base_timestamp,
            duration: Duration::zero(),
        };

        let test_cases = vec![
            TestCase {
                // Test case 0: No current drawdown
                input_equity: EquityPoint {
                    equity: 110.0,
                    timestamp: base_timestamp.add(Duration::days(1)),
                },
                expected_drawdown: Drawdown {
                    equity_range: Range {
                        activated: true,
                        high: 110.0,
                        low: 100.0,
                    },
                    drawdown: 0.0,
                    start_timestamp: base_timestamp,
                    duration: Duration::zero(),
                },
            },
            TestCase {
                // Test case 1: Start of new drawdown w/ lower equity than peak
                input_equity: EquityPoint {
                    equity: 100.0,
                    timestamp: base_timestamp.add(Duration::days(2)),
                },
                expected_drawdown: Drawdown {
                    equity_range: Range {
                        activated: true,
                        high: 110.0,
                        low: 100.0,
                    },
                    drawdown: (-10.0 / 110.0),
                    start_timestamp: base_timestamp.add(Duration::days(2)),
                    duration: Duration::zero(),
                },
            },
            TestCase {
                // Test case 2: Continuation of drawdown w/ lower equity than previous
                input_equity: EquityPoint {
                    equity: 90.0,
                    timestamp: base_timestamp.add(Duration::days(3)),
                },
                expected_drawdown: Drawdown {
                    equity_range: Range {
                        activated: true,
                        high: 110.0,
                        low: 90.0,
                    },
                    drawdown: (-20.0 / 110.0),
                    start_timestamp: base_timestamp.add(Duration::days(2)),
                    duration: Duration::days(1),
                },
            },
            TestCase {
                // Test case 3: Continuation of drawdown w/ higher equity than previous but not higher than peak
                input_equity: EquityPoint {
                    equity: 95.0,
                    timestamp: base_timestamp.add(Duration::days(4)),
                },
                expected_drawdown: Drawdown {
                    equity_range: Range {
                        activated: true,
                        high: 110.0,
                        low: 90.0,
                    },
                    drawdown: (-20.0 / 110.0),
                    start_timestamp: base_timestamp.add(Duration::days(2)),
                    duration: Duration::days(2),
                },
            },
            TestCase {
                // Test case 4: End of drawdown w/ equity higher than peak
                input_equity: EquityPoint {
                    equity: 120.0,
                    timestamp: base_timestamp.add(Duration::days(5)),
                },
                expected_drawdown: Drawdown {
                    equity_range: Range {
                        activated: true,
                        high: 120.0,
                        low: 90.0,
                    },
                    drawdown: 0.0,
                    start_timestamp: base_timestamp.add(Duration::days(2)),
                    duration: Duration::zero(),
                },
            },
            TestCase {
                // Test case 5: No current drawdown w/ residual start_timestamp from previous
                input_equity: EquityPoint {
                    equity: 200.0,
                    timestamp: base_timestamp.add(Duration::days(6)),
                },
                expected_drawdown: Drawdown {
                    equity_range: Range {
                        activated: true,
                        high: 200.0,
                        low: 90.0,
                    },
                    drawdown: 0.0,
                    start_timestamp: base_timestamp.add(Duration::days(2)),
                    duration: Duration::zero(),
                },
            },
            TestCase {
                // Test case 6: Start of new drawdown w/ lower equity than peak & residual fields from previous drawdown
                input_equity: EquityPoint {
                    equity: 180.0,
                    timestamp: base_timestamp.add(Duration::days(7)),
                },
                expected_drawdown: Drawdown {
                    equity_range: Range {
                        activated: true,
                        high: 200.0,
                        low: 180.0,
                    },
                    drawdown: (-20.0 / 200.0),
                    start_timestamp: base_timestamp.add(Duration::days(7)),
                    duration: Duration::zero(),
                },
            },
            TestCase {
                // Test case 7: Continuation of drawdown w/ equity equal to peak
                input_equity: EquityPoint {
                    equity: 200.0,
                    timestamp: base_timestamp.add(Duration::days(8)),
                },
                expected_drawdown: Drawdown {
                    equity_range: Range {
                        activated: true,
                        high: 200.0,
                        low: 180.0,
                    },
                    drawdown: (-20.0 / 200.0),
                    start_timestamp: base_timestamp.add(Duration::days(7)),
                    duration: Duration::days(1),
                },
            },
            TestCase {
                // Test case 8: End of drawdown w/ equity higher than peak
                input_equity: EquityPoint {
                    equity: 200.01,
                    timestamp: base_timestamp.add(Duration::days(9)),
                },
                expected_drawdown: Drawdown {
                    equity_range: Range {
                        activated: true,
                        high: 200.01,
                        low: 180.0,
                    },
                    drawdown: 0.0,
                    start_timestamp: base_timestamp.add(Duration::days(7)),
                    duration: Duration::zero(),
                },
            },
        ];

        for (index, test) in test_cases.into_iter().enumerate() {
            drawdown.update(&test.input_equity);
            assert_eq!(drawdown, test.expected_drawdown, "Test case: {:?}", index)
        }
    }

    #[test]
    fn max_drawdown_update() {
        struct TestCase {
            input_drawdown: Drawdown,
            expected_drawdown: Drawdown,
        }

        let base_timestamp = Utc::now();

        let mut max_drawdown = MaxDrawdown::init();

        let test_cases = vec![
            TestCase {
                // Test case 0: First ever drawdown
                input_drawdown: Drawdown {
                    equity_range: Range {
                        activated: true,
                        high: 115.0,
                        low: 90.0,
                    },
                    drawdown: (-25.0 / 110.0),
                    start_timestamp: base_timestamp,
                    duration: Duration::days(2),
                },
                expected_drawdown: Drawdown {
                    equity_range: Range {
                        activated: true,
                        high: 115.0,
                        low: 90.0,
                    },
                    drawdown: (-25.0 / 110.0),
                    start_timestamp: base_timestamp,
                    duration: Duration::days(2),
                },
            },
            TestCase {
                // Test case 1: Larger drawdown
                input_drawdown: Drawdown {
                    equity_range: Range {
                        activated: true,
                        high: 200.0,
                        low: 90.0,
                    },
                    drawdown: (-110.0 / 200.0),
                    start_timestamp: base_timestamp.add(Duration::days(3)),
                    duration: Duration::days(1),
                },
                expected_drawdown: Drawdown {
                    equity_range: Range {
                        activated: true,
                        high: 200.0,
                        low: 90.0,
                    },
                    drawdown: (-110.0 / 200.0),
                    start_timestamp: base_timestamp.add(Duration::days(3)),
                    duration: Duration::days(1),
                },
            },
            TestCase {
                // Test case 1: Smaller drawdown
                input_drawdown: Drawdown {
                    equity_range: Range {
                        activated: true,
                        high: 300.0,
                        low: 290.0,
                    },
                    drawdown: (-10.0 / 300.0),
                    start_timestamp: base_timestamp.add(Duration::days(8)),
                    duration: Duration::days(1),
                },
                expected_drawdown: Drawdown {
                    equity_range: Range {
                        activated: true,
                        high: 200.0,
                        low: 90.0,
                    },
                    drawdown: (-110.0 / 200.0),
                    start_timestamp: base_timestamp.add(Duration::days(3)),
                    duration: Duration::days(1),
                },
            },
            TestCase {
                // Test case 1: Largest drawdown
                input_drawdown: Drawdown {
                    equity_range: Range {
                        activated: true,
                        high: 10000.0,
                        low: 0.1,
                    },
                    drawdown: (-9999.9 / 10000.0),
                    start_timestamp: base_timestamp.add(Duration::days(12)),
                    duration: Duration::days(20),
                },
                expected_drawdown: Drawdown {
                    equity_range: Range {
                        activated: true,
                        high: 10000.0,
                        low: 0.1,
                    },
                    drawdown: (-9999.9 / 10000.0),
                    start_timestamp: base_timestamp.add(Duration::days(12)),
                    duration: Duration::days(20),
                },
            },
        ];

        for (index, test) in test_cases.into_iter().enumerate() {
            max_drawdown.update(&test.input_drawdown);
            assert_eq!(
                max_drawdown.drawdown, test.expected_drawdown,
                "Test case: {:?}",
                index
            )
        }
    }

    #[test]
    fn avg_drawdown_update() {
        struct TestCase {
            input_drawdown: Drawdown,
            expected_avg_drawdown: AvgDrawdown,
        }

        let base_timestamp = Utc::now();

        let mut avg_drawdown = AvgDrawdown::init();

        let test_cases = vec![
            TestCase {
                // Test case 0: First ever drawdown
                input_drawdown: Drawdown {
                    equity_range: Range {
                        activated: true,
                        high: 100.0,
                        low: 50.0,
                    },
                    drawdown: (-50.0 / 100.0),
                    start_timestamp: base_timestamp,
                    duration: Duration::days(2),
                },
                expected_avg_drawdown: AvgDrawdown {
                    count: 1,
                    mean_drawdown: -0.5,
                    mean_duration: Duration::days(2),
                    mean_duration_milliseconds: Duration::days(2).num_milliseconds(),
                },
            },
            TestCase {
                // Test case 1
                input_drawdown: Drawdown {
                    equity_range: Range {
                        activated: true,
                        high: 200.0,
                        low: 100.0,
                    },
                    drawdown: (-100.0 / 200.0),
                    start_timestamp: base_timestamp,
                    duration: Duration::days(2),
                },
                expected_avg_drawdown: AvgDrawdown {
                    count: 2,
                    mean_drawdown: -0.5,
                    mean_duration: Duration::days(2),
                    mean_duration_milliseconds: Duration::days(2).num_milliseconds(),
                },
            },
            TestCase {
                // Test case 2
                input_drawdown: Drawdown {
                    equity_range: Range {
                        activated: true,
                        high: 1000.0,
                        low: 820.0,
                    },
                    drawdown: (-180.0 / 1000.0),
                    start_timestamp: base_timestamp,
                    duration: Duration::days(5),
                },
                expected_avg_drawdown: AvgDrawdown {
                    count: 3,
                    mean_drawdown: (-59.0 / 150.0),
                    mean_duration: Duration::days(3),
                    mean_duration_milliseconds: Duration::days(3).num_milliseconds(),
                },
            },
        ];

        for (index, test) in test_cases.into_iter().enumerate() {
            avg_drawdown.update(&test.input_drawdown);
            assert_eq!(
                avg_drawdown, test.expected_avg_drawdown,
                "Test case: {:?}",
                index
            )
        }
    }
}
