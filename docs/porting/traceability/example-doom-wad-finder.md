# wad-finder.ts — common-pin audit

Frozen: `packages/coding-agent/examples/extensions/doom-overlay/wad-finder.ts`, `4f9e6f8a4`. Native: `crates/pi-coding-agent/examples/extensions/doom_overlay/wad_finder.rs`.

## Export and branch coverage

findWadFile custom path truthiness/leading tilde, bundled-first/default ordered search and lexical resolve; ensureWadFile existing hit, exact GET URL/status/body/write and caught failure. The common pin writes raw response bytes; native download_wad does the same, with no extra decompression branch.

## Proof and limits

`example_doom_overlay_helpers` passes against the current frozen module: `/tmp/triage2-example_doom_overlay_helpers-audit.log`. [Detailed behavioral fixture mapping](extension-examples-o.md). Audit revalidates the module logic and visible output, not live providers or OS-fault behavior. Native embedding/owned snapshot and en-US locale mappings remain within the standing approvals.

Gap: exotic/wrong-typed input, OS faults, numeric extremes and runtime scheduling oddities follow the current parity bar; existing precise discrepancies remain in the linked mapping. No new dependency, API carrier, or semantic deviation.
