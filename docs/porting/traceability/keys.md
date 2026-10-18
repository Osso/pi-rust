# keys.ts traceability

Source: `packages/tui/src/keys.ts` (1404 lines). Rust: `crates/pi-tui/src/keys.rs`.

Differential oracle: `oracle/tui-leaves-oracle.ts` (`keys_parse`, `keys_match`, `keys_decode`). Tests: `crates/pi-tui/tests/keys.rs`, `crates/pi-tui/tests/differential_keys.rs`, plus the Windows Terminal helper unit test in `src/keys.rs`.

## Exported symbols

| TypeScript export | Rust item | Tests |
|---|---|---|
| `setKittyProtocolActive(active)` | `set_kitty_protocol_active(bool)` | `kitty_protocol_state_round_trips` |
| `isKittyProtocolActive()` | `is_kitty_protocol_active() -> bool` | `kitty_protocol_state_round_trips` |
| `type KeyId` | gap: compile-time string union; Rust accepts `&str` key IDs | n/a |
| `const Key` | gap: compile-time autocomplete helper; no runtime behavior | n/a |
| `type KeyEventType` | `enum KeyEventType { Press, Repeat, Release }` | `key_event_type_is_exported` |
| `isKeyRelease(data)` | `is_key_release(&str) -> bool` | unit + differential |
| `isKeyRepeat(data)` | `is_key_repeat(&str) -> bool` | unit + differential |
| `matchesKey(data, keyId)` | `matches_key(&str, &str) -> bool` | 20 unit tests + differential matrix |
| `parseKey(data)` | `parse_key(&str) -> Option<String>` | unit + differential corpus |
| `decodeKittyPrintable(data)` | `decode_kitty_printable(&str) -> Option<String>` | unit + differential |
| `decodePrintableKey(data)` | `decode_printable_key(&str) -> Option<String>` | unit + differential |

## Internal behavior

| TypeScript item | Rust item | Coverage |
|---|---|---|
| `SYMBOL_KEYS`, `MODIFIERS`, `LOCK_MASK`, codepoint constants | constants | all matching tests |
| `normalizeKittyFunctionalCodepoint` | `normalize_kitty_functional_codepoint` | keypad tests |
| `normalizeShiftedLetterIdentityCodepoint` | `normalize_shifted_letter_identity_codepoint` | shifted/modifyOther tests |
| `LEGACY_KEY_SEQUENCES`, shift/ctrl maps, `LEGACY_SEQUENCE_KEY_IDS` | constant slices + `legacy_sequence_key_id` | `parses_legacy_sequences`, differential |
| `parseKittySequence` (CSI-u, arrows, functional, home/end) | `parse_kitty_csi_u`, `parse_kitty_sequence` | kitty tests, differential |
| `parseEventType` / `_lastEventType` | gap: event suffix is validated but not stored; the source writes `_lastEventType` and never reads it | no observable behavior |
| `matchesKittySequence` | `matches_kitty_sequence` | kitty/base-layout tests |
| `parseModifyOtherKeysSequence` | `parse_modify_other_keys_sequence` | modifyOther tests |
| `matchesModifyOtherKeys` | `matches_modify_other_keys` | modifyOther tests |
| `isWindowsTerminalSession` | `is_windows_terminal_session` + pure `windows_terminal_session` | unit test `windows_terminal_detection_requires_local_session` |
| `matchesRawBackspace` | `matches_raw_backspace` | backspace tests, differential |
| `rawCtrlChar` | `raw_ctrl_char` | ctrl symbol tests |
| `isDigitKey` | `is_digit_key` | digit tests |
| `matchesPrintableModifyOtherKeys` | `matches_printable_modify_other_keys` | modifyOther printable tests |
| `formatKeyNameWithModifiers` | `format_key_name_with_modifiers` | super/combined parse tests |
| `parseKeyId` | `parse_key_id` | all `matches_key` tests |
| `formatParsedKey` | `format_parsed_key` | parse tests |
| `KITTY_CSI_U_REGEX`, `decodeModifyOtherKeysPrintable` | `parse_kitty_csi_u`, `decode_modify_other_keys_printable` | decode tests |

## Behavioral notes

- Global Kitty state is an `AtomicBool`; the TypeScript module-level mutable variable maps directly.
- `matchesKey` lowercases the whole key ID and splits on `+`; modifier order in `parseKey` output is always shift, ctrl, alt, super.
- Legacy ambiguity is mode-aware: `\x1b `, `\x1bB`, `\x1bF`, `\x1b<letter>`, `\x1b<control>`, `\n`, and `\x1b\r` change meaning when Kitty protocol is active.
- `data.length` checks use UTF-16 length (`pi_js_compat::string::utf16_len`) so astral characters keep JavaScript behavior (`\x1b` + emoji is length 3 and is not parsed as an alt key).
- `String.fromCharCode` semantics for symbol checks and printable decode are reproduced with `char_from_utf16_code` (low 16 bits; surrogate results are not symbol keys).
- Raw `0x08` is plain backspace unless `WT_SESSION` is set locally (no `SSH_CONNECTION`/`SSH_CLIENT`/`SSH_TTY`); then it is ctrl+backspace. Empty environment values are treated as unset (`Boolean(process.env.X)`).

## Gaps / exclusions

- `Key` and `KeyId`: TypeScript-only compile-time helpers (autocomplete and typo checking). They have no runtime behavior; Rust callers pass the same key-ID strings (`"ctrl+c"`, `"shift+enter"`, ...). No behavior is lost.
- `_lastEventType`: assigned in `parseKittySequence` but never read by any exported function. Rust validates the event-type field but does not store it.
- `KeyEventType` values are exposed as an enum; the parsed event type is not returned by `parseKey` in either implementation. `isKeyRelease`/`isKeyRepeat` inspect the raw input as in the source.
- Windows Terminal detection is environment-dependent. Differential tests run without `WT_SESSION`; the local-vs-SSH branch is covered by the pure helper unit test. JavaScript callers passing non-string data/key IDs are excluded by the typed API (decision 2026-08-12).

## Discrepancies

None found. All differential parse/match/decode cases agree with the pinned source.
