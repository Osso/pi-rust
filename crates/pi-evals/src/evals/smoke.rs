//! Port of `packages/evals/src/smoke.eval.ts`.
//!
//! The eval case runs as a Rust integration test; this module keeps the
//! harness factory the source file defines.

use pi_coding_agent::core::sdk::NoTools;

use crate::pi_harness::{PiCodingAgentHarness, PiCodingAgentHarnessOptions, create_pi_coding_agent_harness};

pub const EVAL_SET: &str = "Pi Coding Agent smoke";

/// `createPiCodingAgentHarness({ noTools: "all" })`.
pub fn create_smoke_harness(mut options: PiCodingAgentHarnessOptions) -> PiCodingAgentHarness {
    options.no_tools = Some(NoTools::All);
    create_pi_coding_agent_harness(options)
}
