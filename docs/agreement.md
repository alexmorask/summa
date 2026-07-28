# Agreement

_Status: stub — deep dive pending._

**Purpose:** the per-customer timeline of which Policy applied over which
interval. "What was this customer billed under on March 3rd?" becomes a plain
query over history instead of a bolted-on audit log.

**To decide in the deep dive:** how policy changes are represented (upgrades,
downgrades, mid-period changes), how overlaps and backdating work, and how the
timeline feeds Rating.
