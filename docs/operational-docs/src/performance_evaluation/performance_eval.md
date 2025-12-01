# Performance evaluation

While building Emerald we have put an emphasiz on performance testing with the goal of understanding it's limits. 
While tuning the execution client and malachite will be very application dependant, we wanted to get a best-case baseline
to understand any potential overhead introduced by applications built on top of Emerald. 


# Baseline

Emerald was built on top of Malachite, which is a high performant version of the Tendermint consensus algorithm.
To understand the overhead of putting an execution client on top of it, we first benchmarked Malachite without 
Reth. 

We ran a dummy channel based application on top of malachite, which was only generating blocks of a certain size,
and forwarding them to peers for voting. 
The channel based application was initially used to build the skeleton of an Emerald application. 

Malachite core does not support variable block sizes in it's channel based example app, and we added this code ourselves :
[https://github.com/informalsystems/malachite/pull/6]



# Experiment setup

## Malachite

Block size variation : 1kb, 1mb, 2mb
Number of nodes: 4, 8, 10 nodes in one region vs. 4, 8, 10 nodes geodistributed. 

Server setup: 8GB, 4CPUs Digital Ocean droplets


<div style="text-align: left;">  
    <img src="../images/perf/malachite_4_nodes_one_region_block_size.png" width="30%" /> <br/>
    4 nodes in a single region, running Malachite with a varying block size. On 4 nodes, the average block time for 1MB blocks is 133ms.
    
</div>

<div style="text-align: left;">  
    <img src="../images/perf/malachite_8_nodes_geo_block_size.png" width="30%" />  <p>
   Comparison of the performance of Malachite in a geodistributed setup, on 8 nodes, where each 2 nodes were in a different datencter (NYC, LON, AMS). The geodistribution adds certian overhad on consensus and we see that for 1MB blocks, there are spikes up to 260ms. </p>
    
</div>
<div style="text-align: left;">   
    <img src="../images/perf/malachite_10_nodes_geo_vs_local.png" width="30%" />
    We also increased the number of nodes to 10, but did not observe a big decrease in block time compared to running on 8 nodes. 

</div>



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