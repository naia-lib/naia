/// Prints an E2E-debug trace line to stderr (E2E-debug only).
#[cfg(feature = "e2e_debug")]
#[macro_export]
macro_rules! e2e_trace {
    ($($arg:tt)*) => {
        eprintln!($($arg)*);
    };
}

#[cfg(not(feature = "e2e_debug"))]
#[macro_export]
macro_rules! e2e_trace {
    ($($arg:tt)*) => {};
}
