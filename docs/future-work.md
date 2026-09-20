# Future work

These items are explicitly outside the KGI v2 requirement and are not open v2 architecture questions.

## KGI v2.1 candidate list

- low-frequency Live VSPC consistency probe;
- administrative/API-triggered recovery through Supervisor;
- investigate deterministic per-level slot ordering derived from block data,
  aiming for stable coordinates where instances share the same retained DAG;
  v2 retains allocation order, and the ordering and migration consequences
  need a separate design;
- an explicit offline administrative import from a KGI v1 database into a
  separate KGI v2 database, not an in-place migration; network validation,
  resumability, and transactional publication remain to be designed;
- historical graph-window response caching, if usage measurements justify it;
  v2 serves historical windows with indexed, width-capped database reads and
  does not cache their responses. Any future cache must account for VSPC reorgs
  and graph changes within one API publication epoch;
- other improvements must be recorded explicitly rather than silently entering
  the v2 implementation.
