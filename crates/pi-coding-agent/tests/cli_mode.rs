use std::fmt::Debug;

use pi_coding_agent::cli::args::Mode;

fn assert_mode_traits<T: Debug + Clone + Copy + PartialEq + Eq>() {}

#[test]
fn mode_exposes_exact_literals_and_traits() {
    assert_mode_traits::<Mode>();

    let modes = [(Mode::Text, "text"), (Mode::Json, "json"), (Mode::Rpc, "rpc")];

    for (mode, literal) in modes {
        assert_eq!(mode.as_str(), literal);
        let copied = mode;
        assert_eq!(copied, mode);
    }

    assert_ne!(Mode::Text, Mode::Json);
    assert_ne!(Mode::Json, Mode::Rpc);
    assert_ne!(Mode::Text, Mode::Rpc);
}
