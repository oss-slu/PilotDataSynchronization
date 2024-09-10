#include "XPLMPlugin.h"
#include "XPLMUtilities.h"
#include <string.h>

// Plugin Start Function
PLUGIN_API int XPluginStart(char* outName, char* outSig, char* outDesc) {
    strcpy(outName, "Hello World Plugin");
    strcpy(outSig, "example.helloworld");
    strcpy(outDesc, "A Hello World plugin for X-Plane.");

    XPLMDebugString("Hello, World! Plugin has started!\n");
    return 1;
}

// Plugin Stop Function
PLUGIN_API void XPluginStop(void) {
    XPLMDebugString("Hello, World! Plugin has stopped.\n");
}

// Plugin Enable Function
PLUGIN_API int XPluginEnable(void) {
    return 1;
}

// Plugin Disable Function
PLUGIN_API void XPluginDisable(void) {}

// Plugin Receive Message Function
PLUGIN_API void XPluginReceiveMessage(XPLMPluginID inFromWho, int inMessage, void* inParam) {}

