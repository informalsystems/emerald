# Proof-of-Authority Model

Emerald adopts a proof-of-authority (PoA) model in which a set of known, trusted institutions serve as validators, anchoring the network’s security and governance in real-world accountability rather than anonymous resource competition.

## Key Properties

**Institutional Alignment.** Emerald is designed for networks where validators are known organizations that use their real-world reputations as stake. 
This model naturally fits governance models based on identification, accountability, and trust relationships.

**Predictable Performance.** Fixed, permissioned validator sets enable stable block times, low latency, and consistent throughput.
As a result, Emerald avoids the variability of anonymous, resource-competitive consensus mechanisms.

**Clear Governance.** Governance decisions involve identifiable entities, making upgrades, membership changes, and dispute resolution straightforward.
This enables institutions to structure governance to match legal, operational, or regulatory requirements.

**Credibility-Backed Security.** Security is rooted in real-world accountability rather than anonymous resource expenditure.
Network misbehavior can be directly attributed to specific entities, increasing system integrity.

## PoA Module

The Emerald PoA module consists of two main components:

- An EVM smart contract (`ValidatorManager.sol`) that keeps track of the set of validators together with their voting power. 
  The contract provides access control to an `owner` account for updating the validator set of the Emerald network. 
  This includes adding validators by specifying their public keys and voting powers, removing validators, and updating the voting power of existing validators. 
- The wiring that enables Emerald to pass the validator set from the execution layer to the consensus engine. 
  After every finalized block (on `AppMsg::Decided`), Emerald queries the EVM state by calling the `getValidator` view function of the `ValidatorManager` contract and updates its local state. 
  Then, it informs Malachite of the new validator set for the next height. 
