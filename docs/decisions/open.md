# Open architecture decisions

These items require an Architecture decision. They are not implementation
freedom and do not weaken any settled contract.

## Optional destructive administration

Decide whether KGI v2 needs a separate explicit administrative database reset.
Any accepted operation must not introduce a persistent
`--reinitialize-db --yes` startup option that erases data again on every
unattended service restart. Normal `--initialize-db` remains idempotent and
non-destructive for a compatible initialized database.
