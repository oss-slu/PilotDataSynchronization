#!/bin/bash

# run this from the project root
# if you do not have permission to run it, remember to chmod +x the script file
url='https://developer.x-plane.com/wp-content/plugins/code-sample-generation/sdk_zip_files/XPSDK411.zip'
mkdir -p ./lib
curl ${url} > sdk.zip
unzip sdk.zip -d ./lib
rm sdk.zip
