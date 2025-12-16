//! Renacer CLI entry point - thin wrapper around library

fn main() {
    match renacer::cli::run() {
        Ok(code) => std::process::exit(code),
        Err(e) => {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }
    }
}
