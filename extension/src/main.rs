fn main() {
    if let Err(error) = braintrust_lambda_extension::run() {
        eprintln!("Braintrust Lambda extension failed: {error:#}");
        std::process::exit(1);
    }
}
