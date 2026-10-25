# Child response — frozen 4f9e6f8a4

Source `extensions/agents-core/src/child-response.ts`; native `agents_core/child_response.rs` and dispatch integration.

| Export / branch | Proof |
| --- | --- |
| `readCurrentChildAssistantText`: exclude pre-prompt message identities | Runtime spawn RED returned inherited answer as current result; GREEN returns no result for an empty new reply. Native child messages now use Arc identity; production bound sessions cache branch messages by session entry ID, not structural equality or previous array length |
| Last current assistant stopReason=error, nonempty message or default error | Actual dispatch test verifies failed lifecycle and original/default error, instead of completed with old text |
| Reverse scan, concatenate only text with newline, skip empty text, undefined without current text | Nine real frozen differential vectors: empty/no-new/empty-new, equal-valued distinct identities, earlier current text before empty terminal reply, original/default error, inherited error exclusion, exact unpaired UTF-16 |
| Production inherited branch getter | Factory behavioral test checks repeated reads contain no new assistant result, then a prompt produces the fresh result |

Commands: `cargo test -j3 -p pi-coding-agent --test suite_core_1 child_response_ignores` (RED `/tmp/agents-core-child-response-red.log`, GREEN `/tmp/agents-core-child-response-green2.log`); `... frozen_child_response` (`/tmp/agents-core-child-response-diff2.log`). Existing 12 production factory tests compile/pass after mechanical trait conversion (`/tmp/agents-core-child-response-factories.log`); strengthened inherited-identity assertion is pending its focused run.

The Arc representation is required by this frozen built-in identity consumer; it is not a JS shared-mutation API. Other owned-return approvals remain unchanged. Gap: adversarial/runtime-fault input shapes follow the project boundary.
