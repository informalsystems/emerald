# CHANGELOG

## Unreleased

### FEATURES

- Scripts can now generate setup for more than 4 nodes ([#136](https://github.com/informalsystems/emerald/pull/136))
- Height replay mechanism automatically recovers when Reth is behind Emerald's stored height after a crash, eliminating the need for `--engine.persistence-threshold=0`

### FIXES

- Validator set state is now height-related and can raise a new error when the validator set for a given height is not found in the application state ([#142](https://github.com/informalsystems/emerald/pull/142))

## v0.1.0

### FEATURES

- Contract for validator management. ([#12](https://github.com/informalsystems/emerald/issues/12))
- Allow nodes to sync to higher heights ([#13](https://github.com/informalsystems/emerald/issues/13))
- Use secp256k1 for signing ([#36](https://github.com/informalsystems/emerald/issues/36))
- Config flags to enalbe reth pruning and emerald pruning support ([#76](https://github.com/informalsystems/emerald/issues/76))
- CLI to manipulate validator set ([#79](https://github.com/informalsystems/emerald/issues/79))
- Reth validation is adapted to accept multiple blocks with the same timestamp ([#109](https://github.com/informalsystems/emerald/issues/109))
- Reth is forced to persist every block ([#125](https://github.com/informalsystems/emerald/pull/125))
- Fee address recepient is now configurable ([#134](https://github.com/informalsystems/emerald/pull/134))

