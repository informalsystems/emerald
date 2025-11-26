# Consensus Engine

Emerald leverages [Malachite](https://github.com/circlefin/malachite) as its consensus engine. 
Malachite is the most optimized and lightweight evolution of the [Tendermint](https://arxiv.org/abs/1807.04938) Byzantine Fault Tolerant (BFT) protocol, 
which is the most battle-tested consensus protocol in blockchain today. 
It separates consensus from execution, allowing modular development and easy component customization.

> TODO
> - what is a consensus engine providing 
> - what are the benefits of Malachite 
> - how is Malachite integrated in Emerald (Channel API)