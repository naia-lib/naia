macro_rules! l {
    ($target:ident, $line:expr) => {
        $target.push_str($line);
    };

    ($target:ident, $line:expr, $($param:expr),*) => {
        $target.push_str(&format!($line, $($param,)*));
    };
}
