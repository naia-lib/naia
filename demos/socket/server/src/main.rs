#[macro_use]
extern crate log;

use log::LevelFilter;
use simple_logger::SimpleLogger;

mod app;
use app::App;

fn main() -> Result<(), log::SetLoggerError> {
    SimpleLogger::new().with_level(LevelFilter::Info).init()?;

    let mut app = App::new();
    loop {
        app.update();
    }
}
