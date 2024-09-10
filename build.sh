#!/bin/bash

# Define the source and output file
SOURCE_FILE="hello_world.cpp"
OUTPUT_FILE="mac_64/HelloWorldPlugin.xpl"

# Create the output directory if it doesn't exist
mkdir -p mac_64

# Compile the source file
clang -dynamiclib -o $OUTPUT_FILE \
  -I/Users/harshithathota/Documents/SDK/CHeaders/XPLM \
  -I/Users/harshithathota/Documents/SDK/CHeaders/Widgets \
  -F/Users/harshithathota/Documents/SDK/Libraries/Mac \
  -framework XPLM \
  -framework XPWidgets \
  -DAPL=1 \
  $SOURCE_FILE

# Ensure the .xpl file has execute permissions
chmod +x $OUTPUT_FILE
