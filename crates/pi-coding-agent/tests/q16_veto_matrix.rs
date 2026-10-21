#[path = "support/oracle.rs"]
mod oracle;
use pi_agent_core::agent::AgentMessage;
use pi_ai::request::BoxFuture;
use pi_coding_agent::core::session_control_db::*;
use pi_coding_agent::core::session_manager::{SessionEntry, SessionManager};
use pi_coding_agent::supervisor::main::{
    SupervisorInstructionReviewer, SupervisorSession, process_supervisor_request_with_reviewer,
};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
struct Session {
    manager: Arc<Mutex<SessionManager>>,
    feedback: Arc<Mutex<Vec<Value>>>,
    instructions: String,
}
impl SupervisorSession for Session {
    fn abort(&self) -> BoxFuture<'static, Result<(), String>> {
        Box::pin(async { Ok(()) })
    }
    fn prompt(&self, _: String) -> BoxFuture<'static, Result<(), String>> {
        let value = json!({"role":"assistant","content":[{"type":"toolCall","id":"call","name":"supervisor_response","arguments":{"kind":"continue","instructions":self.instructions,"reason":"proof"}}],"api":"faux","provider":"faux","model":"test","stopReason":"toolUse","timestamp":0,"usage":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":0,"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"total":0}}});
        self.manager
            .lock()
            .unwrap()
            .append_message(serde_json::from_value::<AgentMessage>(value).unwrap())
            .unwrap();
        Box::pin(async { Ok(()) })
    }
    fn get_branch(&self) -> Vec<SessionEntry> {
        self.manager.lock().unwrap().get_branch(None)
    }
    fn get_leaf_id(&self) -> Option<String> {
        self.manager.lock().unwrap().get_leaf_id().map(str::to_owned)
    }
    fn append_policy_feedback(&self, content: String, details: Value) -> BoxFuture<'static, Result<(), String>> {
        self.feedback.lock().unwrap().push(
            json!({"customType":"supervisor_policy_feedback","content":content,"display":false,"details":details}),
        );
        Box::pin(async { Ok(()) })
    }
}
#[tokio::test]
async fn full_veto_decision_and_feedback_matrix_matches_frozen() {
    let mut ts = oracle::TsOracle::spawn("q16-runtime-oracle.ts", "packages/coding-agent/src/supervisor/main.ts");
    let decisions = [
        "accept",
        "reject_task_assignment",
        "reject_implementation_prescription",
        "reject_sequencing_instruction",
        "reject_agent_or_tool_direction",
        "reject_plan_override",
        "invalid",
        "throw",
        "generic",
    ];
    for (index, decision) in decisions.iter().enumerate() {
        let instructions = if *decision == "generic" {
            "Continue working toward the active goal."
        } else {
            "Use tool X next"
        };
        let rustdir = tempfile::tempdir().unwrap();
        let tsdir = tempfile::tempdir().unwrap();
        let db = rustdir.path().join("control.sqlite").display().to_string();
        let expected=ts.request(&json!({"id":index,"operation":"vetoScenario","cases":[{"db":tsdir.path().join("control.sqlite"),"kind":"goal_idle_review","instructions":instructions,"decision":decision}]}));
        assert_eq!(expected["ok"], true, "{expected}");
        let id = post_supervisor_request(
            &db,
            &PostSupervisorRequestInput {
                kind: SupervisorRequestKind::GoalIdleReview,
                payload: Default::default(),
                project_id: "pi".into(),
                sender_session_id: "main".into(),
                deadline_at: "2099-01-01T00:00:00.000Z".into(),
            },
        )
        .unwrap();
        let request = claim_next_supervisor_request(&db, "owner").unwrap().unwrap();
        let feedback = Arc::new(Mutex::new(Vec::new()));
        let reviewed = Arc::new(Mutex::new(Vec::new()));
        let captured = reviewed.clone();
        let decision = decision.to_string();
        let reviewer: SupervisorInstructionReviewer = Arc::new(move |instructions, _signal| {
            captured.lock().unwrap().push(instructions);
            let result = if decision == "throw" {
                Err("fake failure".into())
            } else {
                Ok(
                    pi_coding_agent::supervisor::service::parse_supervisor_instruction_review_decision(&json!(
                        decision.to_ascii_uppercase()
                    )),
                )
            };
            Box::pin(async move { result })
        });
        let session = Arc::new(Session {
            manager: Arc::new(Mutex::new(SessionManager::in_memory(None, None).unwrap())),
            feedback: feedback.clone(),
            instructions: instructions.into(),
        });
        process_supervisor_request_with_reviewer(&db, request, session, reviewer)
            .await
            .unwrap();
        let actual = json!({"response":read_supervisor_request(&db,id).unwrap().unwrap().response,"feedback":feedback.lock().unwrap().clone(),"reviewed":reviewed.lock().unwrap().clone()});
        assert_eq!(actual, expected["output"][0], "case {index}");
    }
}
