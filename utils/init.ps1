# Run this script ONLY from the project root

$dlurl="https://developer.x-plane.com/wp-content/plugins/code-sample-generation/sdk_zip_files/XPSDK410.zip"
$download_loc="../sdk.zip"

# Create lib folder if it doesn't exist
New-Item ../lib/SDK -ItemType Directory -Force

# Download the SDK
Invoke-WebRequest -Uri $dlurl -OutFile $download_loc

# Extract SDK into the lib folder
Expand-Archive $download_loc -DestinationPath "../lib/" -Force

# Cleanup
Remove-Item $download_loc