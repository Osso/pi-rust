# Child session — frozen 4f9e6f8a4

Source `extensions/agents-core/src/child-session.ts`; native bound session in `agents_core/session_factories.rs`, shared `ChildAgentSession` contract and dispatch/spawn callers.

| Symbols / branches | Native proof |
| --- | --- |
| `bindProductionChildSession`: abort, prompt, optional drain/transcript delegation | Raw adapter and production factory tests; bound transcript derives from the same native child session manager |
| Live branch `messages` getter | Production bound session reads current branch message entries; native Arc cache keys by entry ID to preserve the [built-in previous-message identity dependency](child-response.md). Strengthened inherited-factory test proves repeated reads produce no new result, then a prompt supplies only fresh text |
| `model`, `thinkingLevel`, `modelRegistry`, `scopedModels`, bound setModel/setThinkingLevel/cycleModel | Bound wrapper now forwards the live mutation target, previously lost at the transcript wrapper. Real production SDK child RED returned no target; GREEN reads model, changes/reads effort, resolves the configured model registry and cycles the single-model list |
| `shutdownChildSession`: await session_shutdown with reason quit, then dispose in finally | Native ChildAgentSession disposal is async Result. Actual frozen binder differential covers success, shutdown rejection, dispose rejection, and both: dispose always executes and its rejection overrides shutdown. Source/native event order and exact error messages match |
| Runtime callers await disposal | Dispatch restore/rejection/admission/finally, spawn rejected commit and background rejected commit await Result; lease listener/abort registration/handles release before final disposal. No sync fire-and-forget compatibility path remains |

RED: `/tmp/agents-core-child-session-dispose-red.log` observed only dispose, not shutdown; `/tmp/agents-core-child-session-model-red2.log` observed missing mutation surface. GREEN: `/tmp/agents-core-child-session-all-green2.log`, all 14 factory tests pass, including four actual frozen disposal vectors and the real configured SDK child.

Mechanical fallout: async disposal propagation in dispatch, registration and spawn commit; raw test fake implements the shutdown contract. Commit rejection still returns the frozen tool result; thrown coordinator/publication/disposal errors propagate rather than being replaced by a synthetic spawn result.

Gap: adversarial entry identities and OS/runtime fault shapes follow the project parity boundary. JS error.stack remains the approved omission.
