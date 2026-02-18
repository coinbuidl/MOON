mod assets;
mod cli;
mod commands;
mod error;
mod logging;
mod moon;
mod openclaw;

fn main() {
    let _ = dotenvy::dotenv();

    if let Err(err) = cli::run() {
        eprintln!("error: {err:#}");
        std::process::exit(1);
    }
}
