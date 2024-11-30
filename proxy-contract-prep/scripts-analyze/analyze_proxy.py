import os 
import re
import csv 
import sys
import json

# step 1: prepare the dataset
json_file = open('./first_contract.json', "r")
data = json.load(json_file)
json_file.close()
print(len(data))

# step 2: initialize variables
# for all the times: including proxy and non-proxy
os.system("rm -r ./stats1")
os.system("mkdir ./stats1")
os.system("rm ./stats1/time.csv")
time_file = open("./stats1/time.csv", "w")
time_writer = csv.writer(time_file)

# for first contract of the proxy
os.system("rm ./stats1/first_proxy.csv")
first_proxy_file = open("./stats/first_proxy.csv", "w")
first_proxy_writer = csv.writer(first_proxy_file)
# for all the proxy
os.system("rm ./stats1/all_proxy.txt")
all_proxy = open("./stats1/all_proxy.txt", "a")

# for first contract of the upgradable
os.system("rm ./stats1/first_upgradable.csv")
first_upgradable_file = open("./stats/first_upgradable.csv", "w")
first_upgrdable_writer = csv.writer(first_upgradable_file)
# for all the upgradable
os.system("rm ./stats1/all_upgradable.txt")
all_upgradable = open("./stats1/all_upgradable.txt", "a")


# for first contract of the non-upgradable
os.system("rm ./stats1/first_non_upgradable.csv")
first_non_upgradable_file = open("./stats/first_non_upgradable.csv", "w")
first_non_upgrdable_writer = csv.writer(first_non_upgradable_file)
# for all the upgradable
os.system("rm ./stats1/all_non_upgradable.txt")
all_non_upgradable = open("./stats1/all_non_upgradable.txt", "a")

# timeouts ones
timeouts_hex_path = "./stats1/timeout_hex/"
os.system("rm -r ./stats1/timeout_hex/")
os.system("mkdir ./stats1/timeout_hex/")
os.system("rm ./stats1/timeout.txt")
timeout = open("./stats1/timeout.txt", "a")

# not_executing hex
not_executing_hex_path = "./stats1/not_executing_hex/"
os.system("rm -r ./stats1/not_executing_hex/")
os.system("mkdir ./stats1/not_executing_hex/")
os.system("rm ./stats1/not_executing.txt")
not_executing = open("./stats1/not_executing.txt", "a")

# counting
timeouts_number = 0
all_timeouts_number = 0
not_executings_number = 0
all_not_executings_number = 0
firt_proxy_number = 0
all_proxy_number = 0
first_upgradble_number = 0
all_upgradable_number = 0
first_non_upgradble_number = 0
all_non_upgradable_number = 0
unique_contract_number = 0
all_contract_number = 0

# iterate all the files
path = "../version1/files"
original_hex_path = "../contracts/"
content_list = os.listdir(path)
file_list = [f for f in content_list if os.path.isfile(path+'/'+f)] 
# print(len(file_list))
for filename in file_list:
    final_filename = filename.split(".")[0]
    # print(final_filename)
    file = open(path + "/" + filename)  
    for line in file:
        line = line.rstrip()
        if "index:" in line:
            # count
            unique_contract_number += 1
            # get info
            splits = re.split(':', line)
            contract = splits[2].split()[0]
            timecost = splits[3].split()[0]
            res = splits[4].split()[0]
            upgradable = splits[5].split()[0]
            # find all the related contracts
            allcontracts = data[contract]
            all_contract_number += len(allcontracts)
            # for proxy
            if res == "Yes":
                # record time cost
                time_writer.writerow([timecost, "yes"])
                # get the proxy number
                firt_proxy_number += 1
                all_proxy_number += len(allcontracts)
                first_proxy_writer.writerow([contract, len(allcontracts)])
                for tempt_addr in allcontracts:
                    addr = tempt_addr.split("/")[1].split(".")[0]
                    all_proxy.write(addr + "\n")
                # get the upgradable proxy number
                if upgradable == "True":
                    first_upgradble_number += 1
                    all_upgradable_number += len(allcontracts)
                    first_upgrdable_writer.writerow([contract, len(allcontracts)])
                    for tempt_addr in allcontracts:
                        addr = tempt_addr.split("/")[1].split(".")[0]
                        all_upgradable.write(addr + "\n")
                else:
                    first_non_upgradble_number += 1
                    all_non_upgradable_number += len(allcontracts)
                    first_non_upgrdable_writer.writerow([contract, len(allcontracts)])
                    for tempt_addr in allcontracts:
                        addr = tempt_addr.split("/")[1].split(".")[0]
                        all_non_upgradable.write(addr + "\n")
            else:
                time_writer.writerow([timecost, "no"])
            # timeouts and notexecutings
            if "Error" in line:
                if "Timeout" in line: 
                    timeouts_number += 1
                    all_timeouts_number += len(allcontracts)
                    timeout.write(contract + "\n")
                    os.system("cp " + original_hex_path + final_filename + "/" + contract + ".hex " + timeouts_hex_path)
                if "NotExecuting" in line:
                    not_executings_number += 1
                    all_not_executings_number += len(allcontracts)
                    not_executing.write(contract + "\n")
                    os.system("cp " + original_hex_path + final_filename + "/" + contract + ".hex " + not_executing_hex_path)
    file.close()

# close all the files
timeout.close()
not_executing.close()
time_file.close() 
first_proxy_file.close()
all_proxy.close()  
first_upgradable_file.close()
all_upgradable.close()
first_non_upgradable_file.close()
all_non_upgradable.close()

 
# print all the info 
print("unique_contract_number", unique_contract_number)
print("     timeouts_number", timeouts_number)
print("     not_executings_number", not_executings_number)
print("     firt_proxy_number", firt_proxy_number)
print("     first_upgradble_number", first_upgradble_number)
print("     first_non_upgradble_number", first_non_upgradble_number)
print()
print("all_contract_number", all_contract_number)
print("     all_timeouts_number", all_timeouts_number)
print("     all_not_executings_number", all_not_executings_number)
print("     all_proxy_number", all_proxy_number)
print("     all_upgradable_number", all_upgradable_number)
print("     all_non_upgradable_number", all_non_upgradable_number)
