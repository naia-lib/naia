use actix_files::Files;
use actix_web::{middleware, web, App, HttpServer};

mod websocket;
use websocket::ws_index;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    std::env::set_var("RUST_LOG", "actix_web=info");

    env_logger::init();

    HttpServer::new(|| {
        App::new()
            .wrap(middleware::Logger::default())
            .service(web::resource("/livereload/").route(web::get().to(ws_index)))
            .service(Files::new("/", "./dist/").index_file("index.html"))
    })
    .bind("127.0.0.1:3113")?
    .run()
    .await
}
