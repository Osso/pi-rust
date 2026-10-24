# Clipboard-image traceability

Frozen `coding-agent/src/utils/clipboard-image.ts`, common pin4f9e6f8a4,301 lines. [Contract](../../specs/clipboard-image.md). This whole-module revalidation supersedes the532f7d12 historical table and pid/nanos/SIGKILL discrepancies; neither was an approved new-pin deviation.

## Capability matrix

| Frozen symbol/branch | Native mapping | Proof |
|---|---|---|
| `ClipboardImage`; constants:15–20 | owned bytes +JsString MIME; same preferences/timeouts/50MiB | full module matrix; direct explicit sync-limit differential |
| `isWaylandSession:22–24`, true/false/empty/exact-case | `is_wayland_session`, lossless dispatch values | helpers13 MIME inputs/7 environments, including raw high surrogate |
| `baseMimeType:26–28`; `extensionForImageMimeType:30–43` | shared JsString split/trim/lowercase; literal MIME→extension arms | all four types, unsupported/empty/parameter strings, FEFF vs NEL, unmatched units |
| `selectPreferredImageMimeType:45–60`; supported test:62–65 | same normalized raw preference then image/* choice | Wayland preference/FEFF/NEL/invalid UTF8; xclip preferred duplicate/fallback; unsupported conversion cases |
| `convertToPng:71–87`, success/decode failure | shared Photon decode and PNG encode, **no EXIF rotation** | BMP and JPEG-with-EXIF full PNG bytes match actual frozen Photon; undecodable input→null |
| `runCommand:89–116`, success/nonzero/spawn failure/timeout/maxBuffer | one shared `spawn_sync` authority; explicit options | module missing/list/read/error cases,50MiB stdout and stderr real overflow, SIGTERM marker; direct13-case result/error probe |
| standard Node byte stdout:107–114 | native byte capture, explicit Buffer UTF8 decoding only where source does it | real subprocess stdout bytes; invalid UTF8 list fixture. Wrongly-typed monkeypatched Node return values are excluded typed-runtime inputs, not a fallback API |
| `readClipboardImageViaWlPaste:118–141` | exact list/read argv, selected raw type and nonempty guard | empty/no-image/failure/preference/raw-NUL-validation/overflow cases |
| `isWSL:143–155` | dispatch env truthiness, then `/proc/version` byte decode/lowercase/caught read failure | both env flags and real shared-host non-WSL release path; caught read-error mapping source-audited |
| PowerShell:161–211 | shared Node tmpdir, UUIDv4, same quoting/script, exact `ok`, file read and best-effort unlink | success/empty/wslpath failure/empty path/PS nonzero/missing+empty file/FEFF/NUL-validation caught; exact argv and file cleanup. UUID normalized only after v4-format validation |
| xclip:213–238 | same TARGETS/preferred-plus-supported attempts, including duplicate preferred | Wayland xclip/preferred duplicate/fallback JPEG cases, exact attempt order |
| native:240–252 | optional native module; has-image/bytes/empty/error | absent/false/empty/has failure/get failure/byte-success fixtures; failures retain original backend error and exact message |
| Linux ordering:254–271 | Wayland/WSL wl→xclip; headless wl only; WSL PowerShell; native only when not Wayland | full module dispatch matrix, source/native full traces |
| public `readClipboardImage:273–301` | Termux short-circuit, Linux/non-Linux dispatch, conversion/passthrough | Termux truthy/empty; Linux/Darwin/win32 dispatch via fake native module; supported passthrough +successful/failed conversion |

## Gaps / boundaries

| Frozen branch | Classification / authority |
|---|---|
| `convertToPng:72–75`: unavailable/unloadable Photon WASM→null | **gap**, approved parent2026-10-24: native Photon is statically linked, so this packaging condition cannot arise. No absence seam/fallback. Analogous to approved embedded package.json/themes deviation2026-10-18 in AGENTS.md. Successful conversion is still tested against real frozen Photon, exact PNG bytes. |
| napi clipboard module replacement | Existing [clipboard-native gap](clipboard-native.md); parent2026-10-24 confirms no widening. Clipboard-image forwards module errors without message changes; native backend errors are its own and remain attached to CallbackError. napi-only codes/branding cannot arise natively and belong to clipboard-native's boundary, not a new clipboard-image shim. No real desktop clipboard/syscall access in proofs. |
| Windows backend execution | Existing project-wide Windows gap. Fake win32 dispatch exercises the source decision only, not Windows native syscalls. |

## Removed old-only paths

- Native→xclip fallback on non-Wayland removed. Its old positive test is replaced by source-proven no-call/null behavior; xclip-preference test now explicitly enters the Wayland branch.
- Missing headless `!DISPLAY` wl-paste branch restored.
- pid/nanoseconds filename replaced by existing UUIDv4 authority.
- Private async stdout-only/SIGKILL runner deleted; shared configured sync authority handles timeout/maxBuffer/result errors. No compatibility adapter remains for the deleted old sync API.
- Reused image-convert helper's EXIF rotation removed from clipboard's private conversion; frozen clipboard conversion does not rotate.

## Evidence

Logs and exact revision/proof invalidations: `/tmp/pi-rust-repin-lossless-interactive/clipboard-module-proof-ledger.md`.

Genuine REDs: `clipboard-module-red.log` (dispatch/UUID/50MiB stderr/SIGTERM), `clipboard-helpers-red.log` (FEFF/NEL), `clipboard-conversion-red.log` (3×2→2×3 unwanted rotation). `clipboard-module-whole-green.log` proves34 module cases +scalar helper matrix before final readability/parent integration; final gates tracked separately. Source oracle imports the actual frozen module with a fake napi dependency and executes real private tool processes; native re-execs into the same private scripts. Complete image bytes, errors, argv/order, signals and file cleanup are compared. No assertion stripping or weakening.

The earlier four-case assigned tmpdir root proof remains in [interactive consumers](interactive-mode.md#assigned-tmpdir-consumers-2026-10-24); it is not substituted for this whole-module proof.

### Queue8 checkpoint gates

Code4f91fa30; parent70b56d1b merged. Composite20 bounded tests GREEN:34 module scenarios,13 direct sync cases,13 MIME/7 environment helper inputs,12 legacy clipboard tests and five sync/caller/tmpdir tests. `clipboard-module-checkpoint.log` is the final matrix after the lint-only hex fixture correction; `clipboard-differentials-corrected.log` retains final raw helper/direct sync proof. Native implementation unchanged after7fd24501. Workspace all-target/keep-going JSON0 errors/0 warnings; package all-target warning-mode clippy0 owned findings/1693 other warnings; strict pinned tsgo/Biome/fmt +Python fixture syntax PASS; inventoryPASS686/186679 statuses442/19/50/175. Manual changed-line Rust/TS readability audit; metrics binary unavailable. Shared random_uuid's preexisting entropy-failure expect is outside this module and retained as a separate main-owned final-gate item, not an added error carrier or claimed entropy proof (parent2026-10-24: do not block checkpoint).
