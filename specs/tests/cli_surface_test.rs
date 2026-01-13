mod common;
use common::ParityTest;

#[test]
fn parity_help() {
    ParityTest::new("help").run();
}

#[test]
fn parity_help_flag() {
    ParityTest::new("--help").run();
}

#[test]
fn parity_short_help_flag() {
    ParityTest::new("-h").run();
}

#[test]
fn parity_coverage() {
    ParityTest::new("coverage").run();
}

#[test]
fn parity_stats() {
    ParityTest::new("stats").run();
}

#[test]
fn parity_invalid_command() {
    ParityTest::new("invalid-command").run();
}
