# Launch Production Network

This guide is designed for network coordinator (companies, foundations, or organizations) that want to launch a new Emerald network.

> [!IMPORTANT]
> Emerald is under active development and should be used at your own risk. 
> The software has not been externally audited, and its security and stability are not guaranteed.

Network coordinators are typically responsible for:

- Recruiting and onboarding external validators to participate in the network
- Collecting validator public keys from participants
- Generating and distributing network genesis files
- Coordinating network launch and operations

Starting a new network involves coordinating with external validator operators:

1. **Recruit Validators**: Identify organizations or individuals who will run validator nodes on the network
2. **Distribute Instructions**: Share the key generation steps with each validator (see [Creating Network Genesis](genesis.md#creating-network-genesis))
3. **Collect Public Keys**: Each validator generates their private keys securely on their own infrastructure and provides the coordinate with their **public key only**
4. **Generate Genesis Files**: Use the collected public keys to create the network genesis files
5. **Distribute Genesis Files**: Share the genesis files with all validators so they can start their nodes
6. **Coordinate Launch**: Ensure all validators start their nodes and connect to each other

## Security Notes

- Validators should **never** share their private keys with anyone. They only provide their public keys for inclusion in the genesis file.
- Validators should ensure no ports are exposed to the internet and all traffic is secured with VPCs or VPN tunnels.

## Roles and Responsibilities

| **Task** | **Who Does It** | **What They Share** |
|----------|-----------------|---------------------|
| Generate validator private keys | Each validator (independently) | Nothing - keep private! |
| Extract and share public keys | Each validator | Public key only (0x...) |
| Collect all public keys | Network coordinator | N/A |
| Generate genesis files | Network coordinator | Genesis files to all validators |
| Generate PoA admin key | Network coordinator | Nothing - keep private! |
| Distribute genesis files | Network coordinator | Both genesis files to all |
| Configure and run Reth node | All participants | Peer connection info |
| Configure and run Emerald node | All participants | Peer connection info |