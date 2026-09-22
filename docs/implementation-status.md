# Implementation status

Updated 22 September 2026.

The architecture documentation scaffold was created from the 17 September KGI v2 handoff. A new consolidated 20 September handoff incorporates the later processing, storage, lifecycle, and API review decisions. No production Rust implementation, tests, or migrations have started.

Implementation has not started. The architecture baseline is committed at
`c51559e`, but its review found that the consolidated handoff needs
reconciliation against the accepted decisions in
`docs/chats/initial_chat.md` and later Architecture decisions. The approved
post-H1 reconciliation report has been applied to the consolidated and focused
architecture documents, pending review and a stable commit. The temporary
reconciliation hold in `AGENTS.md` gates production code, tests, and migrations
until the corrected architecture is reviewed and committed as a stable
baseline.

The reconciled Catchup contract now stops synthetic VSPC production at ordinary
Live eligibility and requires bounded fixed-tip block coverage before Live.
VspcProcessor receives component-local Live at ordinary eligibility and starts
notification processing while BlockProcessor remains in Catchup. Focused test
obligations specify the split Live transition, successful-enqueue accounting,
cross-page dedup, network-scaled page caps and timing, continued VSPC progress,
and `Require(Resync)` from current committed database state when coverage fails.

The non-normative [implementation sequence](implementation-sequence.md) records
the proposed work order and prerequisites.
