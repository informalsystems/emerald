# Performance evaluation

While building Emerald we have put an emphasis on performance testing with the goal of understanding its limits. 
While tuning the execution client and the Malachite consensus engine is very application dependant, 
the goal is to get a best-case baseline to understand any potential overhead introduced by applications built on top of Emerald. 

## Malachite

Emerald is built on top of Malachite, a high performant version of the Tendermint consensus algorithm.  
To understand the overhead of running with an execution client, we first benchmark Malachite without Reth. 

We run a simple channel-based application on top of Malachite. The application only generates blocks of a certain size,  
and forwards them to peers for voting. 
As Malachite does not support variable block sizes in its channel based example app, we added this functionality to [our own fork](https://github.com/informalsystems/malachite/pull/6).

### Setup

- Block size: 1KB, 1MB, 2MB
- Deployments: single datacenter and geo-distributed
- Number of nodes: 4, 8, 10
- Hardware setup: 8GB RAM, 4CPUs Digital Ocean droplets

> TODO we need more details on HW setup

### Results

<div style="text-align: left;">  
    <img src="../images/perf/malachite_4_nodes_one_region_block_size.png" width="60%" /> <br/>
    <p class="caption">Single datacenter deployment on 4 nodes, with a varying block size. The average block time is 133 ms.</p>
</div>

> TODO: it's not clear for which block size is the 133ms average

<div style="text-align: left;">  
    <img src="../images/perf/malachite_8_nodes_geo_block_size.png" width="60%" />  
    <p class="caption">Geo-distributed deployment on 8 nodes, with each 2 nodes in a different datacenter (NYC, LON, AMS).
    The geo-distribution impacts performance, with spikes in block times. </p>
</div>

> TODO: 2 nodes in a different datacenter (NYC, LON, AMS) --> we are missing a DC

> TODO: the spikes seem higher thant 260ms

<div style="text-align: left;">   
    <img src="../images/perf/malachite_10_nodes_geo_vs_local.png" width="60%" />
    <p class="caption"> Deployment on 10 nodes, both in a single datacenter and geo-distributed, with 1MB blocks.  
    No significant difference from running on 8 nodes.</p>
</div>

Although the channel-based application deployed on Malachite doesn't have a concept of transactions, 
we can consider “native” Ethereum EOA-to-EOA transfer (i.e., plain ETH sends), which have ~110bytes. 
In this context, 
- a single datacenter deployment on 4 nodes with 1MB blocks and average block time of 133ms results in around **68k TPS** 
- a geo-distributed deployment on 8 nodes with 1MB blocks and average block time of 250ms results in around **36k TPS**.

> TODO: review the estimates for TPS. are the average block times accurate? 
> - 1,000,000 bytes / 110 bytes ~ 9,090 tx per block
> - 9,090 / 0.133 ~ 68,346 TPS
> - 9,090 ÷ 0.25 ~ 36,360 TPS

## Emerald

### Configuration

For optimal performance of a chain, it is important to tune the exeuction engine according to application requirements. 
We wanted to achieve high throughput of transactions while keeping the system stable. By stable we refer to handling incoming transactions without transactions filling up the entire mempool, building blocks fast enough to keep up with consensus and sending data within the network to avoid congestion at the RPC level. 

We have change the following reth startup CLI parameters from their default values:

```yaml
          "--txpool.pending-max-count=50000",
          "--txpool.pending-max-size=500",
          "--txpool.queued-max-count=50000",
          "--txpool.queued-max-size=500",
          "--txpool.basefee-max-count=50000",
          "--txpool.basefee-max-size=500",
          "--txpool.max-account-slots=100000",
          "--txpool.max-batch-size=10000",
          "--txpool.minimal-protocol-fee=0",
          "--txpool.minimum-priority-fee=0",
          "--txpool.max-pending-txns=20000",
          "--txpool.max-new-txns=20000",
          "--txpool.max-new-pending-txs-notifications=20000",
          "--max-tx-reqs=10000",
          "--max-tx-reqs-peer=255",
          "--max-pending-imports=10000",
          "--builder.gaslimit=1000000000",
```

Note, that for your particualr setup this might be suboptimal. These flags allow a very high influx of transactions from one source, they are buffering up to 50000 transactions in the mempool, and gossip them in big batchers. We also incrased the buffer for pending tx notifications to 20000 (from the default of 200). 



### Bare-metal
These experiments evalaute Emerald on 4 bare metal machines in a local and geodistributed setup. 
The goal is to understand the absolute best performance the chain can have.

### Cloud-based experiments
We also ran Emerald on Digital ocean in the following setup: 64GB RAM, 16 shared CPU threads and were running on regular SSDs.

4 nodes geodistributed
4 nodes in one region

8 nodes geodistributed
8 nodes in one reagion. 


### Load generation

We spammed emerald both with standard asset transfer transactions.
<!--> and transacations targetting a SmartContract that simply increment the value of a counter. -->


Transactions were sent to all nodes in parallel, at a rate of 8000txs/sec.

<!--Emerald can sustain more incoming transactions, but we observed that the number of
transactions getting into a block is 8000, regardless of the incoming load. 
-->

We observe a throughput of 8000tx/sec with block sizes of 0.5-1MB. The reported consensus time is averaging 620ms. 
<div style="text-align: left;">  
    <img src="../images/perf/emerald_do_4_8000_block_time.png" width="30%" /> <br/>
    BLock time of 230ms. 
</div>

<div style="text-align: left;">  
    <img src="../images/perf/emerald_do_4_8000_txs_sec.png" width="30%" /> <br/>
    8000tx/sec sustained on a 4DO network. 
</div>

<div style="text-align: left;">  
    <img src="../images/perf/emerald_4_DO_8000_tx_in_block.png" width="30%" /> <br/>
    Number of transactions in block. 
</div>

When pushing at once 8000 transactions via RPC we noticed a lot of disconnects. We therefore sent every `interval` a 
subset of 8000 txs. The default interval is `200ms`, meaning every 200ms we were sending 1600 transactions.

The second batch shows a slight decrease in sustained transactions per second when we decresed the interval to 100ms. 