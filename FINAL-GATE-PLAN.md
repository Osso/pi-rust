# Final gate preparation

Goal: make strict all-target/all-feature clippy pass for pi-js-compat, pi-tui, pi-ai, and pi-agent-core without behavior changes; replace the three specified hostname conversions and matching test expectations with the existing Node byte decoder.

Excluded: other coding-agent changes, behavioral assertion changes, lint suppression, network, push, deploy, and delegation. Report lints that cannot be fixed mechanically without behavior changes.

- [ ] pi-js-compat: consolidate duplicate test modules; strict clippy; format and workspace check; commit.
- [ ] pi-tui: mechanical lint fixes; strict clippy; format and workspace check; commit.
- [ ] pi-ai: mechanical lint fixes; strict clippy; format and workspace check; commit.
- [ ] pi-agent-core: mechanical lint fixes; strict clippy; format and workspace check; commit.
- [ ] Hostname ingress: update three sites and matching tests; format and workspace check; commits per crate.
- [ ] Run each requested crate's existing tests once, with logs; run inventory; report SHA, results, and skipped lints to main.

Proof ledger: command logs in /tmp/pi-repin-final-gate; record revision/scope and invalidation in the final report.
