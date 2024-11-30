# An Empirical Study of Proxy Smart Contracts at Ethereum Ecosystem Scale

In this work, we conduct the first comprehensive study on Ethereum proxies. We organize our data and code into three sections as follows, aligning with the structure of our paper.

* **1. Proxy Contract Preparation.** To collect a comprehensive dataset of proxies, we propose *ProxyEx*, the first framework designed to detect proxy directly from bytecode.
* **2. Logic Contract Preparation.** To analyze the logic contracts of proxies, we extract transactions and traces for extracting logic contracts from all the related proxies.
* **3. Three Research Questions.** In this paper, we conduct the first systematic study on proxies on Ethereum, aiming to answer the following research questions. 
  * RQ1: Statistics. How many proxies are there on Ethereum? How often do proxies modify their logic? How many transactions are executed on proxies?
  * RQ2: Purpose. What are the major purposes of implementing proxy patterns for smart contracts? 
  * RQ3: Bugs and Pitfalls. What types of bugs and pitfalls can exist in proxies? 


## 1. Proxy Contract Preparation
To facilitate proxy contract data collection, we design a system, *ProxyEx*, to detect proxy contracts from contract bytecode.

#### Environment Setup
First make sure you have the following things installed on your system:

* Boost libraries (Can be installed on Debian with apt install libboost-all-dev)

* Python 3.8 

* Souffle 2.3 or 2.4 

Now install the Souffle custom functors and then You should now be ready to run *ProxyEx*.

* run *cd proxy-contract-prep/proxyex/souffle-addon$ && make*


#### Step 1: unzip the contracts for getting bytecode
We collect all the on-chain smart contract bytecode as of September 10, 2023. In total, we have *62,578,635* smart contracts.

* download *contracts.zip* under *proxyex* from [Google Drive](https://drive.google.com/drive/folders/1qcNFNrKk0OFRCciInWwNM1YFz6KeGyE_?usp=sharing) and place it under *proxy-contract-prep*
* run *unzip contracts.zip* under *proxy-contract-prep* ---> generate the contract bytecode under *proxy-contract-prep/contracts*

#### Step 2: run the proxy detection script in parallel
To speed up the detection process, we optimize it by running multiple python scripts.

* run *bash proxy.sh* under *proxy-contract-prep/scripts-run* ---> generate all the results under *proxy-contract-prep/version1*
* run *bash kill.sh* under *proxy-contract-prep/scripts-run* ---> kill all the running scripts

#### Step 3: analyze all the proxy detection results
 We apply *ProxyEx* on the smart contract bytecode with a timeout of *60* seconds; there are *2,031,422* proxy addresses in total (3.25\%). The average detection time of proxy and non-proxy contracts are *14.85* seconds and *3.88* seconds, respectively. 

* download *first_contract.json* under *proxyex* from [Google Drive](https://drive.google.com/drive/folders/1qcNFNrKk0OFRCciInWwNM1YFz6KeGyE_?usp=sharing) and place it under *proxy-contract-prep/scripts-analyze*
* run *python3 analyze_proxy.py* under *proxy-contract-prep/scripts-analyze* ---> generate all the results under *proxy-contract-prep/scripts-analyze/stats1* for later analysis.
* we have already uploaded our analysis results into the *stats1.zip* from [Google Drive](https://drive.google.com/drive/folders/1qcNFNrKk0OFRCciInWwNM1YFz6KeGyE_?usp=sharing); run *unzip stats1.zip* under under *proxy-contract-prep/scripts-analyze*, you will get some results such as *all_proxy.txt* lists all the 2,031,422 proxy addresses.

#### Step 4: manually analyze 1k contracts for accuracy
To evaluate its effectiveness and performance, we randomly sampled 1,000 contracts from our dataset. Our examination revealed *548* proxy addresses and *452* non-proxy addresses.
*ProxyEx* misclassified one proxy as non-proxy (false negative), indicating that our framework achieves *100\%* precision and over *99\%* recall. 

* *proxy-contract-prep/1k.csv* displays our manually checked results of 1,000 randomly sampled contracts

## 2. Logic Contract Preparation
To extract logic contract addresses, we gather all the transaction traces associated with a *DELEGATECALL* sent from the proxy contracts. We collect a 3-tuple *{FromAddr, ToAddr, CallType}* for every trace from Google Bigquery APIs, which we subsequently aggregate into transactions. In total, we collect 172,709,392 transactions for all the 2,031,422 proxy contracts. 

#### Step 1: extract transaction traces
We run the SQL to download all the traces related to all our proxy contracts.

* run *SELECT * FROM `bigquery-public-data.crypto_ethereum.traces` WHERE from_address IN ( SELECT trace FROM `moonlit-ceiling-399321.gmugcp.traces` ) or to_address IN ( SELECT trace FROM `moonlit-ceiling-399321.gmugcp.traces` ) ORDER BY transaction_hash*; in particular, *moonlit-ceiling-399321.gmugcp.traces* is the table consisting all the proxy contract addresses from *proxy-contract-prep/scripts-analyze/stats1/all_proxy.txt*. 
* the total transaction traces cost around 1.3 TB storage, and we cannot upload all of them here. We choose a segment of the data and store it in "logic-contract-prep/data/sample.json"
* you can fetch all the data using the url *https://storage.googleapis.com/tracesdata/xxx.json*, where *xxx* starts from *000000000000* to *000000004429*.


#### Step 2: extract logic contracts
We aggregate the transaction traces into transactions and obtain the related logic contracts for every proxy contract, sorted by the timestamp (block number).

* run "analyze.py" under *logic-contract-prep/scripts-analyze* ---> generate all the results under *logic-contract-prep/scripts-analyze/impl.json*
* however, the impl.json costs 30GB, which is too large to be put here; therefore, we generate a sample *logic-contract-prep/scripts-analyze/sample_impl.json*
* also, you can fetch the whole *impl.json* from [Google Drive](https://drive.google.com/drive/folders/1qcNFNrKk0OFRCciInWwNM1YFz6KeGyE_?usp=sharing)


## 3. Three Research Questions
#### RQ1 - Statistics
We do some statistics analysis of proxy contracts including bytecode duplication, transaction count and lifespan.
* Bytecode Duplication: run "iv_rq1_figure3.py" under *three-research-questions/rq1/script/* relies on "iv_rq1_figure3.csv" data file under *three-research-questions/rq1/data/* ---> generates figure 3 in the paper.
* Transaction Count: run "iv_rq1_figure4.py" under *three-research-questions/rq1/script/* relies on "iv_rq1_figure4.txt" data file under *three-research-questions/rq1/data/* ---> generates figure 4 in the paper.
* Lifespan: run "iv_rq1_figure5.py" under *three-research-questions/rq1/script/* relies on "iv_rq1_figure5.txt" data file under *three-research-questions/rq1/data/* ---> generates figure 5 in the paper.

#### RQ2 - Purposes
We conduct manual analysis to understand purpose of proxy contracts and categorized into four following types.
* Upgradeability: run "v_rq2_figure6.py" under *three-research-questions/rq2/script/* relies on "v_rq2_figure6.txt" data file under *three-research-questions/rq2/data/* ---> generates figure 6 in the paper.
* Extensibility: The 32 contracts that identified by the detection algorithm of extensibility proxies are listed in "extensibility_proxies.txt", among which one proxy, `0x4deca517d6817b6510798b7328f2314d3003abac`, is the vulnerability proxy with proxy-logic collision bug (labelled by "Audius Hack").
* Code-sharing: The file "code_sharing.txt" contains the 1,137,317 code-sharing proxies and 3,309 code-sharing proxy clusters that we identified.
* Code-hiding: The file "code_hiding.txt" contains the 1,213 code-hiding proxies that we identified. The first column in the csv file is the proxy address while the second column contains a list of tuple: `claimed logic address in EIP1967 slot`, `actual logic address in execution`, `the block where such discrepancy is observed`.

#### RQ3 - Bugs and Pitfalls

In RQ3 we conduct a semi-automated detection of bugs and pitfalls in proxies. 
We leverage a set of automated helpers (as described in the paper) to help us prune non-vulnerable contracts before manual inspection. 
The automated helpers can be found in `pitfall-detection-helpers` folder. 
Note that the final results are obtained faithfully using manual inspection. The helper scripts are only used to data processing to reduce human efforts.

* Proxy-logic collision: 
  - the file "proxy_logic_collision.txt" shows the 32 proxies that we identified as well as our manual inspection results. 
  - the file "proxy_logic_collision_detector_evaluation_sampled_txs.txt" lists the 100 transactions sampled to evaluate the reliability of our automated helper which identifies storage slot read/write operations.
* Logic-logic collision: 
  - the file "logic_logic_collision.txt" contains the 15 proxies that we identified to have logic-logic collisions. 
  - the file "logic_logic_collision_detector_evaluation_sampled_contract_pairs.csv" lists the 100 new-version/old-version logic contract pairs sampled to evaluate the reliability of our automated helper to identify storage collisions between two logic contracts.
* Uninitialized contract: 
  - the file "uninitialized.csv" contains 183 proxies that was not initialized in the same transaction of deployment and may be at risk of front-running attack. Whether they are still exploitable (i.e., re-initialize by malicious actors at present) is also labelled in the csv.
  - the file "identified_initialize_function_calldata.csv" lists the 100 logic contracts sampled to evaluate the quality of `initialize` calldata extracted by our automated helper. 
