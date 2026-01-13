use crate::common::ParityTest;

mod common;

#[test]
fn parity_check_orphans() {
    ParityTest::new("check-orphans").run();
}
