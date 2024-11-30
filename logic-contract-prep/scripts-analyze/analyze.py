# get proxy and related implementations: list of {txhash, impl address, timestamp} 
import os
import json
import sys
import time
from iteration_utilities import unique_everseen

# get all proxy
all_proxy = dict()
file = open("../../proxy-contract-prep/scripts-analyze/stats1/all_proxy.txt", "r")
for line in file:
    all_proxy[line.rstrip()] = ""
print(len(all_proxy))

# sort the traces based on transaction for every file, 
TIMESTAMP = 18112972 # Sep-11-2023 11:59:59 AM +UTC
tx_traces = dict() # key is transaction hash and value is the list of traces
path = "../data/"
start = time.time()
impls = dict() # key is proxy, value is the list of impl info -> json: {txhash, impl address, timestamp} 
impl_hash = dict() # key is proxy, value is the directory of txhash to remove the duplication
for i in range(4430):
    tx_traces = dict()
    filename = str(i).rjust(12, '0') + ".json"
    file1 = open(path + "/" +filename)  
    for line in file1:
        line = line.rstrip()
        json_data = json.loads(line)
        
        # check timestamp
        block_number = int(json_data["block_number"])
        if block_number > TIMESTAMP:
            continue

        # sort the traces by transaction hash
        if "transaction_hash" in json_data.keys():
            txhash = json_data["transaction_hash"].lower()
            if txhash in tx_traces.keys():
                tx_traces[txhash].append(json_data)
            else:
                tx_traces[txhash] = [json_data]
    
    # generate the implementation contracts
    # counter = 0
    for txhash in tx_traces:
        traces = tx_traces[txhash]
        # print(len(traces), traces)
        for trace2 in traces:
            if "call_type" in trace2.keys() and trace2["call_type"] == "delegatecall":
                dele_input = trace2["input"]
                dele_trace = trace2["trace_address"].split(",")
                # print("1. find the delegatecall", trace2)
                target_trace1 = {}
                is_delegatecall = False
                for trace1 in traces:
                    if "call_type" in trace1.keys() and trace1["call_type"] == "call":
                        first_input = trace1["input"]
                        if dele_input == first_input:
                            if "trace_address" not in trace1.keys():
                                is_delegatecall = True
                                target_trace1 = trace1
                                break
                            else:
                                first_trace = trace1["trace_address"].split(",")
                                if len(dele_trace) == len(first_trace) + 1 and dele_trace[:-1] == first_trace:
                                    is_delegatecall = True
                                    target_trace1 = trace1
                                    break
                # print("2. find the related call", target_trace1)
                if is_delegatecall:
                    from_addr = trace2["from_address"]
                    if from_addr in all_proxy.keys():
                        to_addr = trace2["to_address"]
                        block_number = trace2["block_number"]
                        tempt_json = {
                            "tx": txhash,
                            "impl": to_addr,
                            "block": int(block_number),
                        }                      

                        if from_addr in impls.keys():
                            hashes = impl_hash[from_addr]
                            if txhash not in hashes.keys():
                                hashes[txhash] = 0
                                # for impl_hash
                                impl_hash[from_addr] = hashes
                                # for impls
                                impls[from_addr].append(tempt_json)
                        else:
                            # for impls
                            impls[from_addr] = [tempt_json]
                            # for impl_hash
                            hashes = dict()
                            hashes[txhash] = 0
                            impl_hash[from_addr] = hashes
    file1.close()

# transaction count
counter = 0
for txhash in sorted(impls, key=lambda txhash: len(impls[txhash]), reverse=False):
    counter += 1
    if counter > 100:
        break
    print(txhash, len(impls[txhash]))

impl_file = open("impl.json", "a")
count = 0 
for txhash in sorted(impls, key=lambda txhash: len(impls[txhash]), reverse=False):
    values = sorted(impls[txhash], key=lambda k: k["block"])
    tempt_res = {
        "proxy": txhash,
        "impls": values,
    }
    tempt_json = json.dumps(tempt_res)
    impl_file.write(tempt_json+"\n")
    count += 1
    if count % 1000 == 0:
        print(count, txhash, len(values), values[0])
impl_file.close()
