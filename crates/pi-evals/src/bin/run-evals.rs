//! Native `npm run eval` equivalent; always runs Cargo.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = match pi_evals::run_evals::run(&args) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("{error}");
            1
        }
    };
    std::process::exit(code);
}
