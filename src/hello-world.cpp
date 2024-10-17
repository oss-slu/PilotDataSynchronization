#ifdef IBM
    #include <GL/gl.h>
#elif APL
    #include <OpenGL/gl.h>
#endif

#include <cstring>

// Include the X-Plane SDK headers
#ifdef __cplusplus
extern "C" {
#endif

#include "XPLMPlugin.h"
#include "XPLMDisplay.h"
#include "XPLMGraphics.h"
#include "XPLMUtilities.h"

#ifdef __cplusplus
}
#endif

// Window reference
static XPLMWindowID g_window = nullptr;

// Callback function for drawing the window
void DrawWindowCallback(XPLMWindowID in_window_id, void *in_refcon) {
    // Get the window's coordinates
    int left, top, right, bottom;
    XPLMGetWindowGeometry(in_window_id, &left, &top, &right, &bottom);

    // Set up X-Plane graphics state for 2D drawing
    XPLMSetGraphicsState(0, 0, 0, 0, 1, 1, 0);

    // Display "Hello World" message
    const char *message = "Hello World!";
    float color[] = {1.0f, 1.0f, 1.0f}; // RGB color (normalized 0-1)
    XPLMDrawString(
        color,              // Text color
        left + 10,          // X position
        top - 20,           // Y position
        const_cast<char *>(message), // Message text
        nullptr,            // Word wrap width (null for no wrap)
        xplmFont_Basic      // Font to use
    );
}

// Mouse click callback for the window (optional)
int MouseClickCallback(XPLMWindowID in_window_id, int x, int y, XPLMMouseStatus in_mouse, void *in_refcon) {
    return 0; // Return 1 if we handled the click, 0 otherwise
}

// X-Plane plugin start function
PLUGIN_API int XPluginStart(char *out_name, char *out_sig, char *out_desc) {
    // Plugin name, signature, and description
    std::strcpy(out_name, "HelloWorldPlugin");
    std::strcpy(out_sig, "com.example.helloworld");
    std::strcpy(out_desc, "A Hello World plugin for X-Plane.");
  
    // Create a new X-Plane window
    g_window = XPLMCreateWindow(
        100,                         // Left position
        600,                         // Top position
        400,                         // Right position
        400,                         // Bottom position
        1,                           // Visible (1 for true)
        DrawWindowCallback,          // Draw callback
        nullptr,                     // Key callback (not used)
        MouseClickCallback,          // Mouse click callback
        nullptr                      // Reference pointer (refcon)
    );

    // Write a message to the X-Plane log
   XPLMDebugString("Hello World Plugin started.\n");

    return 1; // Return 1 to indicate successful start
}

// X-Plane plugin stop function
PLUGIN_API void XPluginStop() {
    // Destroy the window if it exists
    if (g_window) {
        XPLMDestroyWindow(g_window);
        g_window = nullptr;
    }

    // Write a message to the X-Plane log
    XPLMDebugString("Hello World Plugin stopped.\n");
}

// X-Plane plugin enable function
PLUGIN_API int XPluginEnable() {
    // This function is called when the plugin is enabled
    XPLMDebugString("Hello World Plugin enabled.\n");
    return 1; // Return 1 to indicate the plugin is enabled
}

// X-Plane plugin disable function
PLUGIN_API void XPluginDisable() {
    // This function is called when the plugin is disabled
    XPLMDebugString("Hello World Plugin disabled.\n");
}

// X-Plane plugin message handler function
PLUGIN_API void XPluginReceiveMessage(XPLMPluginID in_from, int in_msg, void *in_param) {
    // Handle any messages here if needed
}
