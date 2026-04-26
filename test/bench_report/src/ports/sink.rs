use crate::core::assertions::AssertOutcome;
use crate::core::capacity::CapacityEstimate;
use crate::core::grouper::BenchGroup;
use crate::core::model::BenchResult;

pub trait ReportSink {
    fn emit(&self, results: &[BenchResult], groups: &[BenchGroup]);
}

pub trait AssertionSink {
    /// Emit assertion results. Returns `true` if all assertions pass.
    fn emit(&self, outcome: &AssertOutcome) -> bool;
}

pub trait CapacitySink {
    fn emit(&self, estimate: &CapacityEstimate);
}
