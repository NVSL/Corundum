#!/bin/bash

full_path=$(realpath $0)
dir_path=$(dirname $full_path)

echo -e "\nPerformance Results"
cat $dir_path/outputs/perf.csv | perl -pe 's/((?<=,)|(?<=^)),/ ,/g;' | column -t -s,

echo -e "\nScalability Results"
cat $dir_path/outputs/scale.csv | perl -pe 's/((?<=,)|(?<=^)),/ ,/g;' | column -t -s,