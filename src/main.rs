mod assets;
mod cli;
mod commands;
mod env_loader;
mod error;
mod logging;
mod moon;
mod openclaw;

fn main() {
    env_loader::load_dotenv();

    if let Err(err) = cli::run() {
        eprintln!("error: {err:#}");
        std::process::exit(1);
    }
}
