mod cli;
mod content;
mod history;
mod project;

fn main() -> Result<(), cli::AppError> {
    cli::run()
}
