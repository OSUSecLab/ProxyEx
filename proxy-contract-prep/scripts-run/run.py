# 3: run the program in parallel
import os
import sys
import json
import csv
import time

# contract_is_proxy
def run_proxyex(filename):
    # Open all files inside a folder
    path = '../../contracts/' + filename 
    # get the list of transactions 
    content_list = os.listdir(path)
    file_list = [f for f in content_list if os.path.isfile(path+'/'+f)]
    file_list.sort()
    # print how many under 
    gobalf.write(path + " How many " + str(len(file_list)) + "\n")
    average_time = 0.0
    total_time = 0.0
    index = 0
    timeout = 0
    # loop
    for file in file_list:
        index += 1
        contract = file.split(".")[0]
        file_name = path + '/' + file
        start = time.time()
        os.system("./proxyex/proxyex.py --timeout_secs=60 " + file_name)
        timecost = time.time() - start 
        total_time += timecost
        proxy_file = "./.temp/" + contract + "/out/" + "inliner.Proxy.csv"
        if os.path.exists(proxy_file):
             # check the functions 
            csvfile = open(proxy_file, newline='')
            spamreader = csv.reader(csvfile, delimiter='\t', quotechar='\t')
            calleeVars = []
            has_fallback = []
            has_selector = []
            no_fallback_no_selector = []
            for row in spamreader:
                calleeVars.append(row[7])
                if row[1] == "__function_selector__":
                    has_selector = [row[7], row[10]]
                elif row[1] == "fallback()":
                    has_fallback = [row[7], row[10]]
                elif row[1] == "()":
                    has_fallback = [row[7], row[10]]
                else:
                    no_fallback_no_selector = [row[7], row[10]]
            csvfile.close()
            # check upgrdable
            upgrdable_file = "./.temp/" + contract + "/out/" + "inliner.Rule2.csv"
            upgrdable = False
            if os.path.getsize(upgrdable_file) != 0:
                upgrdable = True
            # write the result
            if len(calleeVars) >= 1:
                if len(has_fallback) != 0:
                    gobalf.write("index:" + str(index) + " contract:" + contract + " time:" + str(timecost) + " detect_proxy:" + "Yes" + " upgradble:" + str(upgrdable) + " output:" + "null" + " morethanone:" + str(len(calleeVars)) + " hasfallback" + "\n")
                else:
                    if len(has_selector) != 0:
                        gobalf.write("index:" + str(index) + " contract:" + contract + " time:" + str(timecost) + " detect_proxy:" + "Yes" + " upgradble:" + str(upgrdable) + " output:" + "null" + " morethanone:" + str(len(calleeVars)) + " nofallback_hasselector" + "\n")
                    else:
                        gobalf.write("index:" + str(index) + " contract:" + contract + " time:" + str(timecost) + " detect_proxy:" + "Yes" + " upgradble:" + str(upgrdable) + " output:" + "null" + " morethanone:" + str(len(calleeVars)) + " nofallback_noselector" + "\n")
            else:
                gobalf.write("index:" + str(index) + " contract:" + contract + " time:" + str(timecost) + " detect_proxy:" + "No" + " upgradble:" + str(False) + " output:" + "NoResult" + " morethanone:" + "0" + "\n")
        else:
            nodl_file = "./.temp/" + contract + "/NoDelegatecall.facts"
            if os.path.exists(nodl_file):
                gobalf.write("index:" + str(index) + " contract:" + contract + " time:" + str(timecost) + " detect_proxy:" + "No" + " upgradble:" + str(False) + " output:" + "NoDelegatecall" + " morethanone:" + "0" + "\n")
            else:
                if timecost > 60:
                    gobalf.write("index:" + str(index) + " contract:" + contract + " time:" + str(timecost) + " detect_proxy:" + "No" + " upgradble:" + str(False) + " output:" + "Error-Timeout" + " morethanone:" + "0" + "\n")
                    timeout += 1
                else:
                    gobalf.write("index:" + str(index) + " contract:" + contract + " time:" + str(timecost) + " detect_proxy:" + "No" + " upgradble:" + str(False) + " output:" + "Error-NotExecuting" + " morethanone:" + "0" + "\n")
    gobalf.write("averagetime:" + str(total_time/len(file_list)) + " timeout:" + str(timeout) + "\n")


# Step 1: get all the files
path = "../contracts"
content_list = os.listdir(path)
file_list = [f for f in content_list if os.path.isdir(path+'/'+f)]

# Step 2: split the files for running
split = int(sys.argv[1])
index = int(sys.argv[2])
pershare = int(len(file_list) / split)
start = index*pershare+0
end = pershare*(index+1)
if index == split - 1:
    end = len(file_list)

# step 3: run
for i in range(start, end):
    # open the file
    tempt_file = str(start+1)
    # start running
    os.chdir("../version1/")
    gobalf = open("../version1/files/" + tempt_file + ".txt", "a")
    os.chdir("../version1/results/")
    os.system("mkdir " + tempt_file)
    os.chdir("../version1/results/" + tempt_file)
    os.system("cp -r ../proxyex ./proxyex")
    # run 
    run_proxyex(tempt_file)
    # close the file
    gobalf.close()


