use std::process::ExitCode;

fn main() -> ExitCode {
  sagitario_driver::on_large_stack(run)
}

fn run() -> ExitCode {
  match sagitario_cli::parse_args(std::env::args().skip(1)) {
    Ok(command) => sagitario_cli::execute(&command),
    Err(error) => {
      eprintln!("error: {error}\n\n{}", sagitario_cli::USAGE);

      ExitCode::from(sagitario_cli::EXIT_USAGE)
    }
  }
}
