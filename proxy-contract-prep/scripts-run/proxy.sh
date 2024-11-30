#! /bin/bash
rm -r ../version1
mkdir ../version1/
mkdir ../version1/files
mkdir ../version1/results
for ((i = 0; i < 60; i++))
do
    python3 run.py 60 $i &
done
echo "Running scripts in parallel"
wait # This will wait until both scripts finish
echo "Script done running"