use std::path::Path;

pub fn print_header(title: &str) {
    let blue = "\x1b[0;34m";
    let nc = "\x1b[0m";
    println!("\n{}═══════════════════════════════════════════════════════════════{}", blue, nc);
    println!("{}  {}{}", blue, title, nc);
    println!("{}═══════════════════════════════════════════════════════════════{}\n", blue, nc);
}

pub fn print_warning(msg: &str) {
    let yellow = "\x1b[1;33m"; // YELLOW='\033[1;33m'
    let nc = "\x1b[0m";
    println!("{}⚠{} {}", yellow, nc, msg);
}

pub fn print_success(msg: &str) {
    let green = "\x1b[0;32m"; // GREEN='\033[0;32m'
    let nc = "\x1b[0m";
    println!("{}✓{} {}", green, nc, msg);
}

pub fn print_error(msg: &str) {
    let red = "\x1b[0;31m"; // RED='\033[0;31m'
    let nc = "\x1b[0m";
    println!("{}✗{} {}", red, nc, msg);
}

pub fn print_info(msg: &str) {
    let blue = "\x1b[0;34m";
    let nc = "\x1b[0m";
    println!("{}ℹ{} {}", blue, nc, msg);
}

pub fn basename(path: &Path) -> String {
    path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string()
}
