#!/bin/bash

DOCKERFILE_DIR=$(dirname "$(readlink -f "$0")")/..

URL="http://localhost:8545"

docker build -t contender "$DOCKERFILE_DIR"
if [[ $? -ne 0 ]]; then
  echo "Docker build failed."
  exit 1
fi

CONTEXT=/tmp/contender-report-test
mkdir -p $CONTEXT
chmod 777 $CONTEXT

docker run -v "$CONTEXT:/home/appuser/.contender/reports" \
    contender report $URL

sed -i "s|/home/appuser/.contender/reports|$CONTEXT|g" "$CONTEXT/report-2-2.html"
xdg-open "$CONTEXT/report-2-2.html"
if [[ $? -ne 0 ]]; then
  echo "Failed to open report in browser."
  exit 1
fi
