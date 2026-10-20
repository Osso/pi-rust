# UTF-16 collation support

Source contract: Node's direct `JSON.parse(text).localeCompare(JSON.parse(text))`, under the same host locale. Rust API: `pi_js_compat::collator::locale_compare_utf16(&[u16], &[u16]) -> Ordering`. [Spec](../../specs/utf16-collation.md).

| Branch | Proof |
|---|---|
| Lone high/low surrogate identity vs other units/replacement | `lone_surrogate_collation_matches_node_pair_order` |
| Mixed ASCII/accented strings with unpaired units, valid surrogate pairs/emoji | Same 16 × 16 corpus |
| Host-locale initialization | Same test under inherited Node/Rust environment |
| Existing UTF-8 API | Unchanged source/backend; parent owns integrated regression gates |

A RED stub comparison returned Equal for `"\ud800"` vs `"\ud801"`; real Node returns Less. ICU4X 2.3.1 `compare_utf16` also documents replacing each unpaired surrogate with U+FFFD, so that API cannot satisfy this contract. Safe `rust_icu_ustring::UChar::from(Vec<u16>)` retains the original units and `rust_icu_ucol::UCollator::strcoll` invokes ICU4C's code-unit collation. A per-thread collator uses the existing shared host-locale selection. Initialization failure is explicit, with no alternate backend.

Dependencies are pinned exactly and prerequisites documented in [dependencies](../../dependencies.md): ICU4C development headers/libraries, pkg-config, Clang/libclang; matching ICU4C shared libraries at runtime. Development host ICU4C: 78.3. No host installation or configuration changes.

Development logs: `/tmp/pi-rust-image-generator-proof/shared-red.log` (original pair RED); `collation-green.log` (256 exact Node pairs GREEN). Only focused development tests and formatting run here; parent owns final gates.
