#[path = "support/oracle.rs"]
mod oracle;

use pi_tui::latex::{RenderLatexOptions, render_latex};
use serde_json::{Value, json};

const SOURCE_DIRECTORY: &str = "packages/tui/src";

const SOURCES: &[(&str, bool)] = &[
    ("", false),
    ("hello", false),
    ("x=0", false),
    ("x = y", false),
    ("x\ny", false),
    (r"\frac{1}{2}", false),
    (r"\frac{x^2+1}{x-1}", true),
    (r"x=\frac{-b\pm\sqrt{b^2-4ac}}{2a}", true),
    (r"\frac{\frac{x^2+1}{x-1}-\frac{2x}{x+1}}{\frac{x}{x^2-1}}", true),
    (r"\tfrac{1}{2}", true),
    (r"\dfrac{a}{b}", true),
    (r"e^{\frac{1}{2}}", true),
    (r"\sqrt{x}+\sqrt[3]{x}+\sqrt[n]{x+1}", false),
    (r"\sum_{i=0}^n x_i", true),
    (r"\sum_{i=0}^n x_i", false),
    (r"\int\limits_0^1 f(x)\,dx", true),
    (r"\int\nolimits_0^1 f(x)\,dx", true),
    (r"\lim_{x\to 0}\frac{\sin x}{x}=1", false),
    (r"\min_{x\in X} f(x)", true),
    (r"\operatorname*{arg\,max}_{x\in X} f(x)", false),
    (r"\operatorname*{arg\,max}_{x\in X} f(x)", true),
    (r"\begin{pmatrix}1&200\\3000&4\end{pmatrix}", false),
    (r"\begin{pmatrix}1&200\\3000&4\end{pmatrix}", true),
    (r"\begin{bmatrix}a&b\\c&d\end{bmatrix}", false),
    (r"\begin{vmatrix}a&b\\c&d\end{vmatrix}", false),
    (r"\begin{array}{cc}a&b\\c&d\end{array}", false),
    (r"\begin{aligned}a&=b\\&=c\end{aligned}", false),
    (r"\begin{alignedat}{2}a&=b&\quad c&=d\\e&=f&g&=h\end{alignedat}", false),
    (r"\begin{cases}a & x<0 \\ b & x=0 \\ c & x>0\end{cases}", false),
    (
        r"\begin{cases}a & \text{if }x=0 \\ b & \text{otherwise}\end{cases}",
        false,
    ),
    (
        r"\begin{equation}\begin{split}a&=b\\&=c\end{split}\end{equation}",
        false,
    ),
    (r"\left\lbrace x \middle| x>0 \right\rbrace", false),
    (r"\left.\frac{dy}{dx}\right|_{x=0}", false),
    (r"\overset{!}{=}+\underset{n}{x}+\stackrel{def}{=}", false),
    (r"\binom{n}{k}+\vec{x}+\hat{y}+\overline{AB}", false),
    (r"\mathbb{C}^3 \to \mathbb{C}^3", false),
    (r"\not\subseteq", false),
    (r"\not\in", false),
    (r"\not\approx", false),
    (r"\not x", false),
    (r"\not", false),
    (r"\pmod n", false),
    (r"\pod n", false),
    (r"\bmod", false),
    (r"\boxed{a+b}", false),
    (r"\fbox{a+b}", false),
    (r"\text{hello world}", false),
    (r"\textbf{hi}", false),
    (r"\mathrm{diag}(1,2)", false),
    (r"\mathcal{Z}(\beta)", false),
    (r"\unknown{x}", false),
    (r"\frac{1}{x", false),
    ("x}", false),
    (r"\begin{matrix}1&2", false),
    ("x\\", false),
    ("}", false),
    ("{", false),
    (r"\\", false),
    (r"a\\b", false),
    (r"\begin{cases}\end{cases}", false),
    (r"\begin{pmatrix}\end{pmatrix}", false),
    (r"\begin{array}{cc}\end{array}", false),
    (r"\begin{equation}\end{equation}", false),
    (r"x^{a+b}_{c-d}", false),
    (r"x^{\frac{1}{2}}", false),
    (r"\frac12", false),
    (r"\sqrt2", false),
    (r"\sum_0^1", false),
    (r"\sum\limits_0^1", false),
    (r"\sum\nolimits_0^1", false),
    (r"p_\pm", false),
    (r"f_1^{\text{ut}}", false),
    ("你 + 好", false),
    ("😀+x", false),
    ("a\u{00a0}b", false),
    ("\u{feff}x", false),
    (r"x\in\{0,\pm 1\}", false),
];

const COMMANDS: &[&str] = &[
    "alpha",
    "beta",
    "Gamma",
    "pm",
    "times",
    "cdot",
    "oplus",
    "therefore",
    "le",
    "geq",
    "neq",
    "to",
    "Rightarrow",
    "mapsto",
    "partial",
    "infty",
    "emptyset",
    "langle",
    "rangle",
    "lvert",
    "lVert",
    "lbrace",
    "backslash",
    "sum",
    "prod",
    "int",
    "oint",
    "lim",
    "sin",
    "cos",
    "log",
    "det",
    "max",
    "deg",
    "binom",
    "sqrt",
    "vec",
    "hat",
    "overline",
    "underbrace",
    "operatorname",
    "mod",
    "pmod",
    "boxed",
    "text",
    "mathbb",
    "frac",
    "not",
];

const ATOMS: &[&str] = &[
    "x",
    "1",
    "-2",
    r"\alpha",
    r"\frac{1}{2}",
    r"\sqrt{2}",
    "a+b",
    r"\text{hi}",
    r"\partial",
    r"\infty",
    "x_1^2",
    r"\left(x\right)",
];

fn generated_cases() -> Vec<(String, bool)> {
    let mut cases = Vec::new();
    for command in COMMANDS {
        for atom in ATOMS {
            cases.push((format!("\\{command}{{{atom}}}"), false));
            cases.push((format!("\\{command}{atom}"), false));
            cases.push((format!("x\\{command}{atom}y"), false));
        }
    }

    let environments = [
        "matrix",
        "smallmatrix",
        "pmatrix",
        "bmatrix",
        "Bmatrix",
        "vmatrix",
        "Vmatrix",
        "aligned",
        "align",
        "gather",
        "split",
        "cases",
        "equation",
        "displaymath",
    ];
    for environment in environments {
        cases.push((
            format!("\\begin{{{environment}}}a&b\\\\c&d\\end{{{environment}}}"),
            false,
        ));
        cases.push((
            format!("\\begin{{{environment}}}a&b\\\\c&d\\end{{{environment}}}"),
            true,
        ));
    }

    for source in [
        r"x_{i=0}^{n+1}",
        r"\frac{\frac{a}{b}}{\frac{c}{d}}",
        r"\sum_{n=1}^{\infty}\frac{1}{n^2}",
        r"\begin{pmatrix}\frac{1}{2}&x\\y&\sqrt{z}\end{pmatrix}",
        r"R\left(\frac{\pi}{4}\right)=\begin{pmatrix}\frac{\sqrt{2}}{2}&-\frac{\sqrt{2}}{2}\\\frac{\sqrt{2}}{2}&\frac{\sqrt{2}}{2}\end{pmatrix}.",
        r"a\qquad b\quad c\,d\;e\:f\!g",
        r"x \ne y \ge z \le w \approx v \equiv u",
        r"\mathbf w=\begin{pmatrix}1\\0\end{pmatrix}",
        r"\Psi(x,t)=\sum_{n=1}^{\infty}c_n\sqrt{\frac{2}{L}}",
    ] {
        cases.push((source.to_string(), false));
        cases.push((source.to_string(), true));
    }

    cases
}

#[test]
fn latex_matches_the_pinned_typescript_source() {
    let mut cases: Vec<(String, bool)> = SOURCES
        .iter()
        .map(|(source, display)| ((*source).to_string(), *display))
        .collect();
    cases.extend(generated_cases());

    let requests: Vec<Value> = cases
        .iter()
        .map(|(source, display)| json!({ "source": source, "display": display }))
        .collect();

    let mut oracle = oracle::TsOracle::spawn("latex-oracle.ts", SOURCE_DIRECTORY);
    let response = oracle.request(&json!({
        "id": "latex-render",
        "operation": "latex_render",
        "cases": requests,
    }));
    assert_eq!(response["ok"], true, "oracle failed: {response}");

    let results = response["output"]["results"].as_array().expect("results array");
    assert_eq!(results.len(), cases.len());

    for ((source, display), expected) in cases.iter().zip(results) {
        let actual = render_latex(source, RenderLatexOptions { display: *display });
        let expected = expected.as_str().map(str::to_owned);
        assert_eq!(actual, expected, "latex mismatch for {source:?} (display={display})");
    }
}
