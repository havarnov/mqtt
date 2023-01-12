# A MQTTv5 broker

* implemented in safe Rust (using nom-rs, tokio, etc)
* will support cluster mode


## possible soundness issue with cluster mode

1. client A connects and subscribes to topic T on cluster 1
2. client B sends message with QoS = 2 on topic T to cluster 2
3. client A disconnects
4. cluster 2 forwards message to cluster 1
5. client A connects to cluster 2 and state is transferred
6. cluster 1 processes message
7. ??? client A never sees message