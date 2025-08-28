use skelly::eyre;

#[tokio::main]
async fn main() -> eyre::Result<()> {
    skelly::setup();

    let args = std::env::args().skip(1).collect::<Vec<String>>();
    let args_str: Vec<&'static str> = args
        .into_iter()
        .map(|s| Box::leak(s.into_boxed_str()) as &str)
        .collect();
    let args_slice: &'static [&'static str] = Box::leak(args_str.into_boxed_slice());
    let args: libterm::Args = facet_args::from_slice(args_slice).map_err(|e| e.into_owned())?;

    libterm::load().run(args);

    Ok(())
}
