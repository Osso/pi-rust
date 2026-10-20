use std::path::Path;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let fetch = pi_ai::http::default_fetch();
    let output = Path::new(env!("CARGO_MANIFEST_DIR")).join("data/image-models.json");
    let result = pi_ai::image_catalog_generator::run(
        &args,
        fetch.as_ref(),
        &output,
        &mut std::io::stdout(),
        &mut std::io::stderr(),
    )
    .await;
    if let Err(error) = result {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
