# index.ts frozen exports (Q1)

Reference: immutable `4f9e6f8a4`, 128 lines. Rust package root: `crates/pi-tui/src/lib.rs`.

Concrete frozen `TUI` maps to `TuiRenderer`, not the later shared trait. `index_exports::frozen_root_tui_constructs_and_renders` reproduces missing concrete constructor before the change, then renders through the exported root type on a capturing terminal.

Removed later root reexports: AltScreenFlashContainer; HStack/VStack/Stack options; ScrollView options; Marked/Tokens/Token; renderLatex/RenderLatexOptions; pathToFileURL; TuiMainScreen/render state; TuiAltScreen/options; ViewportTUI/isViewportTUI; TuiMode/TuiStopOptions/input listener types; compositeTuiLine; stripTerminalSequences/getOsc8LinkAtColumn. Internal modules remain for Q2/Q13, explicitly authorized by parent. All affected coding-agent imports now use those internal module paths. No component behavior changed.

Added missing root aliases `RootFlowLayout`/`RootLayoutRect` and `FlowRenderRegionLayout`. `OverlayUnfocusOptions` is represented by the existing Rust `OverlayHandle::unfocus_to(Option<ComponentRef>)` adaptation; no separate struct. KeyId is a String alias; Box maps to BoxComponent. Component/focus traits and event/poll/timer adaptations remain explicit in module traceability.

Remaining frozen root exports are unchanged: autocomplete, component/editor types, fuzzy, keybindings, keys, root compositor, StdinBuffer, terminal error/type, terminal colors, image utilities, container/fixed/render/flow/overlay types, four width/slice/truncate/wrap utilities. Their other module proofs belong to their owning slices; this row proves export mapping, not all component behavior.
