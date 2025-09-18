use skelly::eyre;

#[tokio::main]
async fn main() -> eyre::Result<()> {
    skelly::setup();

    let args: libterm::Args = facet_args::from_std_args()?;
    libterm::load().run(args);

    Ok(())
}
