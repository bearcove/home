#[tokio::main]
async fn main() {
    skelly::setup();

    libdoctor::load().run().await;
}
