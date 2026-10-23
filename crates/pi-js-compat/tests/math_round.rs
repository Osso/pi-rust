use pi_js_compat::number::{js_number_to_string, math_round};
#[test]
fn round_matches_node_at_binary64_ties_and_boundaries() {
    let values = [
        f64::NEG_INFINITY,
        -4503599627370495.5,
        -2.5,
        -1.5,
        -0.5,
        -0.49999999999999994,
        -0.0,
        0.0,
        0.49999999999999994,
        0.5,
        1.5,
        4503599627370495.5,
        f64::INFINITY,
        f64::NAN,
    ];
    let script = "const xs=JSON.parse(process.argv[1]);console.log(JSON.stringify(xs.map(s=>{let n=Math.round(Number(s));return Object.is(n,-0)?'-0':String(n)})))";
    let inputs: Vec<_> = values
        .iter()
        .map(|n| {
            if *n == 0.0 && n.is_sign_negative() {
                "-0".into()
            } else {
                js_number_to_string(*n)
            }
        })
        .collect();
    let output = std::process::Command::new("node")
        .args(["-e", script, &serde_json::to_string(&inputs).unwrap()])
        .output()
        .unwrap();
    assert!(output.status.success());
    let expected: Vec<String> = serde_json::from_slice(&output.stdout).unwrap();
    let actual: Vec<_> = values
        .iter()
        .map(|n| {
            let n = math_round(*n);
            if n == 0.0 && n.is_sign_negative() {
                "-0".into()
            } else {
                js_number_to_string(n)
            }
        })
        .collect();
    assert_eq!(actual, expected);
}
