#![windows_subsystem = "windows"]
use actix_web::{web, App, HttpRequest, HttpResponse, HttpServer};
use alcro::{Content, UIBuilder};
use mime_guess::from_path;
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "examples/actix-embed"]
struct Asset;

async fn assets(req: HttpRequest) -> HttpResponse {
    let path = if req.path() == "/" {
        // if there is no path, return default file
        "index.html"
    } else {
        // trim leading '/'
        &req.path()[1..]
    };

    // query the file from embedded asset with specified path
    match Asset::get(path) {
        Some(content) => HttpResponse::Ok()
            .content_type(from_path(path).first_or_octet_stream().as_ref())
            .body(content.data.into_owned()),
        None => HttpResponse::NotFound().body("404 Not Found"),
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // actix-web runs on tokio, so the server can share alcro's runtime.
    // We specified the port to be 0, meaning the operating system will
    // choose some available port for us; get the first bound address'
    // port, so we know where to point the browser at.
    let server = HttpServer::new(|| App::new().default_service(web::get().to(assets)))
        .bind("127.0.0.1:0")?;
    let port = server.addrs().first().unwrap().port();
    let server = server.run();
    let server_handle = server.handle();
    let server_task = tokio::spawn(server);

    let ui = UIBuilder::new()
        .content(Content::Url(&format!("http://127.0.0.1:{}", port)))
        .size(400, 400)
        .run()
        .await?;

    ui.wait_finish().await;
    // gracefully shutdown actix web server
    server_handle.stop(true).await;
    server_task.await??;
    Ok(())
}
