use crate::common::ParityTest;

mod common;

#[test]
fn parity_check_refs() {
    ParityTest::new("check-refs").run();
}
