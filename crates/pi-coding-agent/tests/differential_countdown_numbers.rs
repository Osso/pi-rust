use crate::shared_oracle as oracle;
use pi_coding_agent::modes::interactive::components::{
    countdown_timer::CountdownTimer, status_indicator::RetryStatusIndicator,
};
use pi_tui::Component;
use serde_json::json;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

#[test]
fn countdown_and_retry_preserve_all_javascript_number_values() {
    pi_coding_agent::modes::interactive::theme::init_theme(Some(&"dark".into()), false).unwrap();
    let mut oracle = oracle::TsOracle::spawn("interactive-components-oracle.ts", "packages/coding-agent/src");
    for (delay, attempt, max_attempts) in [
        ("2500", "1", "3"),
        ("1000.5", "1.5", "3.75"),
        ("0", "-0", "0"),
        ("-0.5", "-1.5", "-3.25"),
        ("-1500", "NaN", "Infinity"),
        ("NaN", "Infinity", "-Infinity"),
        ("Infinity", "1e21", "1e-7"),
        ("-Infinity", "-1e21", "-1e-7"),
        ("9007199254740992000", "9007199254740992", "9007199254740991"),
    ] {
        let expected=oracle.request(&json!({"id":1,"module":"modes/interactive/components/status-indicator.ts","operation":"numericCountdown","delay":delay,"attempt":attempt,"maxAttempts":max_attempts,"width":100}));
        assert_eq!(expected["ok"], true, "{expected}");
        let ticks = Rc::new(RefCell::new(Vec::new()));
        let tick_values = ticks.clone();
        let expired = Rc::new(Cell::new(0));
        let expirations = expired.clone();
        let requests = Rc::new(Cell::new(0));
        let count = requests.clone();
        let render: Rc<dyn Fn()> = Rc::new(move || count.set(count.get() + 1));
        let mut timer = CountdownTimer::new(
            delay.parse::<f64>().unwrap(),
            Some(render.clone()),
            Rc::new(move |seconds: f64| {
                tick_values
                    .borrow_mut()
                    .push(if seconds == 0.0 && seconds.is_sign_negative() {
                        "-0".to_owned()
                    } else {
                        pi_js_compat::number::js_number_to_string(seconds)
                    })
            }),
            Rc::new(move || expirations.set(expirations.get() + 1)),
        );
        let mut retry = RetryStatusIndicator::new(
            render,
            attempt.parse::<f64>().unwrap(),
            max_attempts.parse::<f64>().unwrap(),
            delay.parse::<f64>().unwrap(),
        );
        let mut frames = vec![retry.render(100)];
        let mut active =
            vec![usize::from(timer.next_deadline().is_some()) + usize::from(retry.next_countdown_deadline().is_some())];
        for _ in 0..3 {
            if let Some(deadline) = timer.next_deadline() {
                timer.poll(deadline);
            }
            if let Some(deadline) = retry.next_countdown_deadline() {
                retry.poll_countdown(deadline);
            }
            frames.push(retry.render(100));
            active.push(
                usize::from(timer.next_deadline().is_some()) + usize::from(retry.next_countdown_deadline().is_some()),
            );
        }
        timer.dispose();
        retry.dispose();
        let retired =
            usize::from(timer.next_deadline().is_some()) + usize::from(retry.next_countdown_deadline().is_some());
        assert_eq!(
            json!({"ticks":*ticks.borrow(),"expired":expired.get(),"requests":requests.get(),"frames":frames,"active":active,"retired":retired}),
            expected["output"],
            "delay={delay} attempt={attempt} maxAttempts={max_attempts}"
        );
    }
}
