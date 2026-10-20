//! Native `npm run eval` equivalent; `PI_EVAL_CHILD_PROGRAM` injects a child for tests.

fn main() {
    let program = std::env::var_os("PI_EVAL_CHILD_PROGRAM").unwrap_or_else(|| "cargo".into());
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = match pi_evals::run_evals::run(&args, &program) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("{error}");
            1
        }
    };
    std::process::exit(code);
}
