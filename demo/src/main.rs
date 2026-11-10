//! `cargo run -p ikigai-ui-demo [addr]`: serve the demo on `addr` (default 127.0.0.1:8080).

#[tokio::main(flavor = "current_thread")]
async fn main() -> std::io::Result<()> {
    let addr = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:8080".to_string());
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    println!(
        "ikigai-ui demo on http://{}/ (the gallery, /page/contact, /page/support)",
        listener.local_addr()?
    );
    ikigai_ui_demo::serve(listener, Default::default()).await
}
