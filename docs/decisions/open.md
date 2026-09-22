# Open architecture decisions

These items require an Architecture decision. They are not implementation
freedom and do not weaken any settled contract.

## Genesis-anchored recovery

Specify the recovery contract for an initialized database whose retained
`db_pp` is Genesis:

- distinguish it from a genuinely Empty database;
- define Resync versus Rebuild eligibility before and after boundary sealing;
- define the GetBlocks and VSPC starting anchors; and
- define the resulting lifecycle milestones.

The resolution must preserve the settled representation: Genesis has a
non-null ORIGIN selected-parent identity, zero actual direct parents, and
coordinate `(level = 1, slot = 0)`.

## Optional destructive administration

Decide whether KGI v2 needs a separate explicit administrative database reset.
Any accepted operation must not introduce a persistent
`--reinitialize-db --yes` startup option that erases data again on every
unattended service restart. Normal `--initialize-db` remains idempotent and
non-destructive for a compatible initialized database.
