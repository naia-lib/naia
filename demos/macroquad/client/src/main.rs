use macroquad::prelude::*;

mod app;
mod interp;

use app::App;

#[macroquad::main("NaiaMacroquadDemo")]
async fn main() {
    let mut app = App::new();

    loop {
        app.update();

        next_frame().await
    }
}
