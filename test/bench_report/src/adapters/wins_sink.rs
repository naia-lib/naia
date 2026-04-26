use crate::core::assertions::AssertOutcome;
use crate::ports::sink::AssertionSink;

/// Runs all Win assertions and prints results to stdout.
pub struct WinsSink;

impl AssertionSink for WinsSink {
    fn emit(&self, outcome: &AssertOutcome) -> bool {
        !outcome.failed()
    }
}
