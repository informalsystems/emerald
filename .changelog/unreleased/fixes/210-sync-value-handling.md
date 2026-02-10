- `[app]` Improve sync value handling: return errors on decode failures instead of panicking,
  treat invalid synced payloads as errors (indicates state divergence), and store synced values
  as undecided so block data is available when decided.
  ([\#210](https://github.com/informalsystems/emerald/pull/210))
