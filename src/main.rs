mod cli;
mod content;
mod project;

fn main() -> Result<(), cli::AppError> {
    cli::run()
}
