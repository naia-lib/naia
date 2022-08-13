
macro_rules! l {
    ($target:ident, $line:expr) => {
        #[allow(clippy::format_push_string)]
        $target.push_str($line);
    };

    ($target:ident, $line:expr, $($param:expr),*) => {
        #[allow(clippy::format_push_string)]
        $target.push_str(&format!($line, $($param,)*));
    };
}
