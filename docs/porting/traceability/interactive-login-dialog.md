# login-dialog.ts

Source: `coding-agent/src/modes/interactive/components/login-dialog.ts`
Rust: `crates/pi-coding-agent/src/modes/interactive/components/login_dialog.rs`

| Symbol / branch | Rust item | Proof |
|---|---|---|
| constructor title/name overrides | `LoginDialogComponent::new` | `login_dialog.rs` (override `Prompt Repro`) |
| `signal` (AbortSignal) | `signal()` → `CancellationToken` | cancel test (`is_cancelled`) |
| `replaceInputWithSubmittedText` | `replace_input_with_submitted_text` | previous-input tests (`> value` count) |
| `cancel` (abort, reject, onComplete) | `cancel` | cancel test |
| `showAuth` URL/instructions/click hint/openBrowser/render | `show_auth` | auth-instructions test (injected opener) |
| `showDeviceCode` | `show_device_code` | code-mapped |
| `showManualInput` promise | `show_manual_input` + `LoginInputResultFn` | manual-input test |
| `showPrompt` append/placeholder/input/hints | `show_prompt` | prompt tests |
| `showDetails` | `show_details` | details test |
| `showInfo` links/close hint | `show_info` | info-links test |
| `showWaiting` | `show_waiting` | code-mapped |
| `showProgress` | `show_progress` | code-mapped |
| cancel keybinding | `handle_input` | cancel test |
| submit → resolve + replace input | `handle_input` | prompt tests |
| other input → `Input` | `handle_input` | prompt typing |
| focus propagation | `Focusable` | code-mapped |

Adaptations: `Promise<string>` resolvers become `LoginInputResultFn` callbacks (`Ok` resolves, `Err(LoginError)` rejects). `openBrowser` is injected as `Rc<dyn Fn(&str)>` so tests never launch a browser. `TUI.requestRender` is the narrow `LoginDialogUi` trait.

Gaps: `showDeviceCode`, `showWaiting`, `showProgress`, the `Input.onEscape` wiring (the component intercepts cancel first, so it is unreachable), and the darwin click hint have no direct test.
