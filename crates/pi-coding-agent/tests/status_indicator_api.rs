use pi_coding_agent::modes::interactive::components::status_indicator::{
    BranchSummaryStatusIndicator, CompactionStatusIndicator, CompactionStatusReason, RetryStatusIndicator,
    StatusIndicatorKind, WorkingStatusIndicator,
};
use pi_js_compat::js_json::JsString;
use pi_tui::Component;
use std::{cell::Cell, rc::Rc};

#[test]
fn status_subclasses_expose_inherited_message_and_kind_without_input_handlers() {
    pi_coding_agent::modes::interactive::theme::init_theme(Some(&"dark".into()), false).unwrap();
    let requests = Rc::new(Cell::new(0));
    let count = requests.clone();
    let render: Rc<dyn Fn()> = Rc::new(move || count.set(count.get() + 1));
    let mut working = WorkingStatusIndicator::new(render.clone(), "old", None);
    let mut retry = RetryStatusIndicator::new(render.clone(), 1.0, 2.0, 1000.0);
    let mut compaction = CompactionStatusIndicator::new(render.clone(), CompactionStatusReason::Manual);
    let mut branch = BranchSummaryStatusIndicator::new(render);
    let raw = JsString::from_utf16(vec![0xd800, 0x78, 0xdc00]);
    working.set_message(raw.clone());
    retry.set_message(raw.clone());
    compaction.set_message(raw.clone());
    branch.set_message(raw.clone());
    assert_eq!(working.kind(), StatusIndicatorKind::Working);
    assert_eq!(retry.kind(), StatusIndicatorKind::Retry);
    assert_eq!(compaction.kind(), StatusIndicatorKind::Compaction);
    assert_eq!(branch.kind(), StatusIndicatorKind::BranchSummary);
    for status in [
        &mut working as &mut dyn Component,
        &mut retry,
        &mut compaction,
        &mut branch,
    ] {
        assert!(!status.wants_input());
        assert!(JsString::join(status.render(40), "\n").contains(&raw));
        status.dispose();
    }
    let before = requests.get();
    assert!(!retry.poll_countdown(std::time::Instant::now() + std::time::Duration::from_secs(2)));
    assert_eq!(requests.get(), before);
}
