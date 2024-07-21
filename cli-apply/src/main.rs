use clap::Parser;

#[derive(clap::Parser)]
struct Args {
    source: std::path::PathBuf,
    patch: std::path::PathBuf,
    output: std::path::PathBuf,

    #[arg(long)]
    skip_crc: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let source = std::fs::read(args.source)?;
    let patch = std::fs::read(args.patch)?;

    let start = std::time::Instant::now();
    let contents = ups::apply_patch_with(
        ups::Options {
            skip_crc: args.skip_crc,
        },
        source,
        &patch,
    )?;
    println!("took {:.02?}", start.elapsed());

    std::fs::write(args.output, contents)?;

    Ok(())
}
