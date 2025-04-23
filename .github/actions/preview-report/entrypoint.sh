#!/bin/sh -l

contender report http://localhost:8545
mkdir -p /github/workspace
cp /home/appuser/.contender/reports/* /github/workspace
sed -i 's|/home/appuser/.contender/reports|/github/workspace|g' /github/workspace/*.html
echo "report_path=$report_path" >> $GITHUB_OUTPUT
