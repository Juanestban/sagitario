use std::process::ExitCode;

fn main() -> ExitCode {
  sagitario_driver::on_large_stack(run)
}

fn run() -> ExitCode {
  match sagitario_nova::parse_args(std::env::args().skip(1)) {
    Ok(command) => sagitario_nova::execute(&command),
    Err(error) => {
      eprintln!("error: {error}\n\n{}", sagitario_nova::USAGE);

      ExitCode::from(sagitario_nova::EXIT_USAGE)
    }
  }
}
