use crate::core::model::BenchResult;

pub trait BenchResultSource {
    fn load(&self) -> Vec<BenchResult>;
}
