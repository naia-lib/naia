use crate::common::ParityTest;

mod common;

#[test]
fn parity_validate() {
    ParityTest::new("validate").run();
}
