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

For every accepted head delta, the Web adopts its target
`HeadGraphCoverage`. It removes block and level contents below
`retain_from_level`, while preserving reference-only endpoint metadata still
required by an edge intersecting the visible window. It then applies its
selected display depth within the advertised complete range. A snapshot's
coverage establishes the same initial boundary.

## Fixed views — settled

For a fixed block window `[visible_start_level, visible_end_level]`, continue
delta catch-up while it intersects the target revision's HGC coverage:

```text
visible_end_level >= coverage.retain_from_level
```

When the HGC boundary advances into the window, preserve the prefix below
`retain_from_level` unchanged and apply absolute patches that affect the
covered visible suffix, including crossing-edge endpoint metadata needed by
that suffix. Do not apply an HGC eviction instruction to the preserved prefix.
The view advances its delta cursor after consuming the revision even though
its preserved prefix reflects the last revision that covered it. Track the
coverage boundary so the presentation can distinguish that frozen prefix from
the live suffix.

Once `visible_end_level < coverage.retain_from_level`, stop delta catch-up and
retain the fixed view as frozen; do not silently refresh or recenter it.
Off-window parent endpoints do not extend the fixed block window for this
test. On GraphEpoch change, a fixed view retains its current image marked
frozen/stale. Explicit refresh reruns the original anchor query.

For every successful level, block-hash, or DAA window request, the Web retains
the original anchor and adopts the response's `GraphWindowResolution`.
`resolved_level` is the fixed focus for that image and subsequent deltas do not
re-resolve the original anchor. Explicit refresh resubmits that anchor and
replaces the stored resolution with the new response. In particular, if later
reorgs change a DAA-resolved level's score, keep focus on the resolved level
rather than resolving the original DAA score again.

Fixed-view revision catch-up is distance-adaptive, not a blanket slow path.
Define:

```text
distance = max(0, head_level - visible_window_end_level)
```

If the head is visible or at most ten levels ahead, update without added
throttling. Beyond that, delay increasingly as distance grows, while
preserving contiguous catch-up and snapshot fallback if delta retention
expires. The exact delay curve and cap remain deferred in the
[decision register](../decisions/deferred.md).

## Block identity and Genesis — settled

Web block identity is the hash. Decode HTTP-local integers through the
response dictionary immediately. Do not use storage `CompactId` values in
React state, URL identity, or cross-response comparisons. Level/slot
coordinates are display positions, not global canonical identities.

When a materialized Genesis is present, Web recognizes it from its empty
actual direct-parent list. A pruned non-Genesis PP may have no visible parent
edge but still has actual direct parents; ORIGIN as selected parent does not
by itself mark Genesis.
