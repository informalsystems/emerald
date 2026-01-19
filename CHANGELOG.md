# CHANGELOG

## v0.2.0


Emerald v0.2.0 comes with Fusaka support and a few UX and performance improvements. 

Please check out the CHANGELOG for a detailed list of changes. 

We want to thank Noble for their contribution in turning Emerald into a library. 

### BREAKING CHANGES

- `[reth]` Use Reth 1.9.3.([#138](https://github.com/informalsystems/emerald/pull/138))
- `[state]` App state contains info on the chain configuraton loaded from genesis. Requires path to eth genesis file in config.([#138](https://github.com/informalsystems/emerald/pull/138))

### FEATURES

- `[state/app/engine]` Emerald supports Fusaka and activates it by default from genesis. Replaces ENGINE_GET_PAYLOAD_V4 with ENGINE_GET_PAYLOAD_V5([#138](https://github.com/informalsystems/emerald/pull/138))
- `[app]` Height replay mechanism automatically recovers when Reth is behind Emerald's stored height after a crash, eliminating the need for `--engine.persistence-threshold=0` ([#126](https://github.com/informalsystems/emerald/issues/126))
- `[config]` Command to perform a bulk update to malachite and emerald config ([#143](https://github.com/informalsystems/emerald/pull/143))
- `[config]` Scripts can now generate setup for more than 4 nodes ([#136](https://github.com/informalsystems/emerald/pull/136))
- `[state]` App state contains info on the chain configuraton loaded from genesis. Requires path to eth genesis file in config.([#138](https://github.com/informalsystems/emerald/pull/138))

### FIXES

- `[engine]` Use `ExecutionPayloadEnvelopeV4` for Prague instead of `ExecutionPayloadEnvelopeV3` ([#173](https://github.com/informalsystems/emerald/issues/173))
- `[state/app]` Validator set state is now height-related and can raise a new error when the validator set for a given height is not found in the application state ([#142](https://github.com/informalsystems/emerald/pull/142))

## v0.1.0

### FEATURES

- Allow nodes to sync to higher heights ([#13](https://github.com/informalsystems/emerald/issues/13))
- CLI to manipulate validator set ([#79](https://github.com/informalsystems/emerald/issues/79))
- Config flags to enalbe reth pruning and emerald pruning support ([#76](https://github.com/informalsystems/emerald/issues/76))
- Contract for validator management. ([#12](https://github.com/informalsystems/emerald/issues/12))
- Fee address recepient is now configurable ([#134](https://github.com/informalsystems/emerald/pull/134))
- Reth is forced to persist every block ([#125](https://github.com/informalsystems/emerald/pull/125))
- Reth validation is adapted to accept multiple blocks with the same timestamp ([#109](https://github.com/informalsystems/emerald/issues/109))
- Use secp256k1 for signing ([#36](https://github.com/informalsystems/emerald/issues/36))

