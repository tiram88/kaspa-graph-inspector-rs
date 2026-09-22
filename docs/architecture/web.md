# Web architecture

## Scope and ownership

This document owns browser-side graph state, update behavior, view focus, and
public block identity. [The API architecture](api.md) owns the graph wire
contract, snapshots, deltas, SSE cursor delivery, and response-local hash
dictionaries.

## Update acquisition — settled

The v1 Web's fast repeated polling is replaced by SSE cursor wakeups and HTTP
delta/snapshot catch-up for cached views. The Web keeps one in-flight catch-up
loop and coalesces desired cursors. Because SSE reconnection is not
exactly-once, each wakeup is a desired `(GraphEpoch, revision)` cursor; graph
state comes from HTTP delta or snapshot responses.

Head-following stays prompt. On GraphEpoch change, a head-following view
automatically reloads.

## Fixed views — settled

For a fixed view, continue applying graph revisions **while its entire window
remains inside HGC**. Freeze the last coherent image once it slides outside
HGC; do not silently refresh or recenter it. On GraphEpoch change, a fixed
view retains its coherent image marked frozen/stale. Explicit refresh reruns
the original anchor query.

A DAA request resolves the DAA to a level at request time, then focuses on
that level. If later reorgs change that level's DAA, keep focus on the level;
do not repeatedly resolve the original DAA target again.

Fixed-view revision catch-up is distance-adaptive, not a blanket slow path.
Define:

```text
distance = max(0, head_level - visible_window_end_level)
```

If the head is visible or at most ten levels ahead, update without added
throttling. Beyond that, delay increasingly as distance grows, while
preserving contiguous catch-up and snapshot fallback if delta retention
expires. The exact delay curve and cap remain implementation work.

## Block identity and Genesis — settled

Web block identity is the hash. Decode HTTP-local integers through the
response dictionary immediately. Do not use storage `CompactId` values in
React state, URL identity, or cross-response comparisons. Level/slot
coordinates are display positions, not global canonical identities.

When a materialized Genesis is present, Web recognizes it from its empty
actual direct-parent list. A pruned non-Genesis PP may have no visible parent
edge but still has actual direct parents; ORIGIN as selected parent does not
by itself mark Genesis.
