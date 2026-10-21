#[path = "support/oracle.rs"]
mod oracle;
use pi_coding_agent::core::runtime_mailbox_transcript::read_accepted_runtime_mailbox_store_refs;
use serde_json::{Value, json};
#[test]
fn whole_transcript_complete_lf_raw_refs_and_error_order_match_frozen_helper() {
    let t = tempfile::tempdir().unwrap();
    let path = t.path().join("receipt.jsonl");
    let mut ts = oracle::TsOracle::spawn("control-db-oracle.ts", "packages/coding-agent/src");
    let fixtures = [
        ("{\"type\":\"message\",\"mailboxStoreRefs\":[{}]}\n", true),
        ("null\n", false),
        (
            "{\"type\":\"message\",\"mailboxStoreRefs\":[{\"sessionPath\":\"/partial\",\"messageId\":\"last\"}]}",
            true,
        ),
        ("{broken\n", false),
        (
            "{\"type\":\"message\",\"id\":\"old\",\"mailboxStoreRefs\":[{\"sessionPath\":\"/old\",\"messageId\":\"one\"}]}\n{\"type\":\"branch_summary\",\"parentId\":null}\n{\"type\":\"custom\",\"customType\":\"runtime_mailbox_acceptance\",\"data\":{\"mailboxStoreRefs\":[{\"sessionPath\":\"/new\",\"messageId\":\"two\"}]}}\n",
            true,
        ),
    ];
    for (contents, success) in fixtures {
        std::fs::write(&path, contents).unwrap();
        let response = ts.request(&json!({"id":1,"steps":[{"op":"acceptedRefs","path":path}]}));
        let peer = &response["output"][0];
        assert_eq!(peer["ok"], success, "{response}");
        let rust = read_accepted_runtime_mailbox_store_refs(path.to_str());
        if success {
            assert_eq!(
                serde_json::to_value(rust.expect("TS accepts raw refs")).unwrap(),
                peer["value"]
            )
        } else {
            assert!(rust.is_err(), "TS failure must not be silently skipped: {peer}")
        }
    }
    let absent = t.path().join("absent");
    assert_eq!(
        read_accepted_runtime_mailbox_store_refs(absent.to_str()).unwrap().len(),
        0
    );
    assert_eq!(read_accepted_runtime_mailbox_store_refs(None).unwrap().len(), 0);
    let _: Value = json!(null);
}
