
use crate::app::App;

pub fn start_loop(app: &mut App) {
    loop {
        app.update();
    }
}