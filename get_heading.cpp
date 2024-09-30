// This basic plugin is a modified and de-commented version of the sample
// "Hello, World" plugin code found here:
// https://developer.x-plane.com/code-sample/hello-world-sdk-3/

#include <string.h>
#include <windows.h>

#include <string>

#include "XPLMDataAccess.h"
#include "XPLMDisplay.h"
#include "XPLMGraphics.h"
BOOL APIENTRY DllMain(HANDLE hModule, DWORD ul_reason_for_call,
                      LPVOID lpReserved) {
    switch (ul_reason_for_call) {
        case DLL_PROCESS_ATTACH:
        case DLL_THREAD_ATTACH:
        case DLL_THREAD_DETACH:
        case DLL_PROCESS_DETACH:
            break;
    }
    return TRUE;
}

#ifndef XPLM300
#error This is made to be compiled against the XPLM300 SDK
#endif

# GETTING DATAREFS: 
    # 1. "sim/cockpit2/gauges/indicators/heading_AHARS_deg_mag_pilot"
    # 2. "sim/cockpit2/gauges/indicators/heading_AHARS_deg_mag_copilot"


// An opaque handle to the window we will create
static XPLMWindowID g_window;

static XPLMDataRef elevationMslRef;
static XPLMDataRef elevationAglRef;

// Callbacks we will register when we create our window
void    draw_heading_window(XPLMWindowID in_window_id, void* in_refcon);
int     dummy_mouse_handler(XPLMWindowID in_window_id, int x, int y, int is_down,
                        void* in_refcon) {
    return 0;
}
XPLMCursorStatus dummy_cursor_status_handler(XPLMWindowID in_window_id, int x,
                                             int y, void* in_refcon) {
    return xplm_CursorDefault;
}
int dummy_wheel_handler(XPLMWindowID in_window_id, int x, int y, int wheel,
                        int clicks, void* in_refcon) {
    return 0;
}
void dummy_key_handler(XPLMWindowID in_window_id, char key, XPLMKeyFlags flags,
                       char virtual_key, void* in_refcon, int losing_focus) {}

PLUGIN_API int XPluginStart(char* outName, char* outSig, char* outDesc) {
    strcpy(outName, "GetHeadingsPlugin");
    strcpy(outSig, "hmeiners.getHeadingsPlugin");
    strcpy(outDesc, "A plugin that displays the headings listed by both the pilot's and copilot's instruments.");

    XPLMCreateWindow_t params;
    params.structSize = sizeof(params);
    params.visible = 1;
    params.drawWindowFunc = draw_hello_world;
    params.handleMouseClickFunc = dummy_mouse_handler;
    params.handleRightClickFunc = dummy_mouse_handler;
    params.handleMouseWheelFunc = dummy_wheel_handler;
    params.handleKeyFunc = dummy_key_handler;
    params.handleCursorFunc = dummy_cursor_status_handler;
    params.refcon = NULL;
    params.layer = xplm_WindowLayerFloatingWindows;
    // Opt-in to styling our window like an X-Plane 11 native window
    params.decorateAsFloatingWindow = xplm_WindowDecorationRoundRectangle;

    // Set the window's initial bounds
    int left, bottom, right, top;
    XPLMGetScreenBoundsGlobal(&left, &top, &right, &bottom);
    params.left = left + 50;
    params.bottom = bottom + 150;
    params.right = params.left + 200;
    params.top = params.bottom + 200;

    // Obtain datarefs for MSL and AGL elevation, respectively
    headingPilotRef =   XPLMFindDataRef("sim/cockpit2/gauges/indicators/heading_AHARS_deg_mag_pilot");
    headingCopilotRef = XPLMFindDataRef("sim/cockpit2/gauges/indicators/heading_AHARS_deg_mag_copilot");

    g_window = XPLMCreateWindowEx(&params);

    // Position the window as a "free" floating window, which the user can drag around
    XPLMSetWindowPositioningMode(g_window, xplm_WindowPositionFree, -1);
    // Limit resizing our window: maintain a minimum width/height of 100 boxels and a max width/height of 300 boxels
    XPLMSetWindowResizingLimits(g_window, 200, 50, 300, 50);
    XPLMSetWindowTitle(g_window, "Positional Flight Data");

    return g_window != NULL;
}

PLUGIN_API void XPluginStop(void) {
    XPLMDestroyWindow(g_window);
    g_window = NULL;
}

PLUGIN_API void XPluginDisable(void) {}
PLUGIN_API int  XPluginEnable(void) { return 1; }
PLUGIN_API void XPluginReceiveMessage(XPLMPluginID inFrom, int inMsg, void* inParam) {}

void draw_heading_window(XPLMWindowID in_window_id, void* in_refcon) {
    XPLMSetGraphicsState(0 /* no fog */,            0 /* 0 texture units */,
                         0 /* no lighting */,       0 /* no alpha testing */,
                         1 /* do alpha blend */,    1 /* do depth testing */,
                         0 /* no depth writing */
    );

    int l, t, r, b;
    XPLMGetWindowGeometry(in_window_id, &l, &t, &r, &b);

    float col_white[] = {1.0, 1.0, 1.0};  // RGB

    // Dataref provides altitudes in meters, need to convert to feet to match
    // in-game display for validation
    std::string headingPilotStr =
        "Heading, Pilot: " + std::to_string(headingPilotRef) + " deg";
    std::string headingCopilotStr =
        "Heading, Copilot: " + std::to_string(headingCopilotRef) + " deg";
    XPLMDrawString(col_white, l + 10, t - 20, headingPilotStr.c_str(), NULL,
                   xplmFont_Proportional);
    XPLMDrawString(col_white, l + 10, t - 30, headingCopilotStr.c_str(), NULL,
                   xplmFont_Proportional);
}
