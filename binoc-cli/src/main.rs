fn main() {
    let status = match binoc_cli::run(binoc_stdlib::default_registry(), std::env::args_os()) {
        Ok(status) => status,
        Err(err) => {
            eprintln!("Error: {err}");
            binoc_cli::EXIT_ERROR
        }
    };
    std::process::exit(status);
}
