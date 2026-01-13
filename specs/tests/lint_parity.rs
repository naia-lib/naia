use crate::common::ParityTest;

mod common;

#[test]
fn parity_lint() {
    ParityTest::new("lint")
        //.arg("lint") // Removed redundant arg
        .run();
}
