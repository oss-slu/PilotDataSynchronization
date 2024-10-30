// This basic plugin is a modified and de-commented version of the sample
// "Hello, World" plugin code found here:
// https://developer.x-plane.com/code-sample/hello-world-sdk-3/

#include <string.h>
#include <windows.h>

#include <cmath>
#include <string>

#ifdef __cplusplus
extern "C" {
#endif

#include "XPLMDataAccess.h"
#include "XPLMDisplay.h"
#include "XPLMGraphics.h"

#ifdef __cplusplus
}
#endif

BOOL APIENTRY
DllMain(HANDLE hModule, DWORD ul_reason_for_call, LPVOID lpReserved) {
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

// An opaque handle to the window we will create
static XPLMWindowID g_window;

// DataRef Identifiers
static XPLMDataRef elevationMslRef;
static XPLMDataRef elevationAglRef;
static XPLMDataRef airspeedRef;
static XPLMDataRef verticalVelocityRef;
static XPLMDataRef headingPilotRef;
static XPLMDataRef headingFlightmodelRef;

// Callbacks we will register when we create our window
void draw_pilotdatasync_plugin(XPLMWindowID in_window_id, void* in_refcon);

int dummy_mouse_handler(
    XPLMWindowID in_window_id,
    int x,
    int y,
    int is_down,
    void* in_refcon
) {
    return 0;
}

XPLMCursorStatus dummy_cursor_status_handler(
    XPLMWindowID in_window_id,
    int x,
    int y,
    void* in_refcon
) {
    return xplm_CursorDefault;
}

int dummy_wheel_handler(
    XPLMWindowID in_window_id,
    int x,
    int y,
    int wheel,
    int clicks,
    void* in_refcon
) {
    return 0;
}

void dummy_key_handler(
    XPLMWindowID in_window_id,
    char key,
    XPLMKeyFlags flags,
    char virtual_key,
    void* in_refcon,
    int losing_focus
) {}

PLUGIN_API int XPluginStart(char* outName, char* outSig, char* outDesc) {
    strcpy(outName, "PilotDataSyncPlugin");
    strcpy(outSig, "oss.pilotdatasyncplugin");
    strcpy(outDesc, "A plug-in that collects and transmits X-Plane 11 data.");

    XPLMCreateWindow_t params;
    params.structSize = sizeof(params);
    params.visible = 1;
    params.drawWindowFunc = draw_pilotdatasync_plugin;
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
    elevationMslRef = XPLMFindDataRef("sim/flightmodel/position/elevation");
    elevationAglRef = XPLMFindDataRef("sim/flightmodel/position/y_agl");

    // Obtain datarefs for Airspeed and Vertical Velocity
    airspeedRef = XPLMFindDataRef("sim/flightmodel/position/true_airspeed");
    verticalVelocityRef = XPLMFindDataRef("sim/flightmodel/position/vh_ind");

    // Obtain dataref for Pilot heading and True Magnetic Heading
    headingPilotRef = XPLMFindDataRef("sim/cockpit2/gauges/indicators/heading_AHARS_deg_mag_pilot");
    headingFlightmodelRef = XPLMFindDataRef("sim/flightmodel/position/mag_psi");

    g_window = XPLMCreateWindowEx(&params);

    // Position the window as a "free" floating window, 
    // which the user can drag around
    XPLMSetWindowPositioningMode(g_window, xplm_WindowPositionFree, -1);
    XPLMSetWindowResizingLimits(g_window, 200, 80, 200, 80);
    XPLMSetWindowTitle(g_window, "Positional Flight Data");

    return g_window != NULL;
}


PLUGIN_API void XPluginStop(void) {
    XPLMDestroyWindow(g_window);
    g_window = NULL;
}

PLUGIN_API void XPluginDisable(void) {}

PLUGIN_API int XPluginEnable(void) {
    return 1;
}

PLUGIN_API void
XPluginReceiveMessage(XPLMPluginID inFrom, int inMsg, void* inParam) {}


void draw_pilotdatasync_plugin(XPLMWindowID in_window_id, void* in_refcon) {
    XPLMSetGraphicsState(
        0 /* no fog */,
        0 /* 0 texture units */,
        0 /* no lighting */,
        0 /* no alpha testing */,
        1 /* do alpha blend */,
        1 /* do depth testing */,
        0 /* no depth writing */
    );

    int l, t, r, b;
    XPLMGetWindowGeometry(in_window_id, &l, &t, &r, &b);

    float col_white[] = {1.0, 1.0, 1.0}; // RGB

    // Dataref provides altitudes in meters, need to convert to feet to match
    // in-game display for validation
    float metersToFeetRate = 3.28084;
    float currentElevationMsl =
        XPLMGetDataf(elevationMslRef) * metersToFeetRate;
    float currentElevationAgl =
        XPLMGetDataf(elevationAglRef) * metersToFeetRate;
    float msToKnotsRate = 1.94384;
    float trueAirspeed = XPLMGetDataf(airspeedRef) * msToKnotsRate;
    float currentVerticalVelocity = XPLMGetDataf(verticalVelocityRef);
    float currentPilotHeading = XPLMGetDataf(headingPilotRef);
    float currentFlightmodelHeading = XPLMGetDataf(headingFlightmodelRef);

    // Create strings from DataRefs to display in plugin window
    std::string elevationMslStr =
        "Elevation (MSL): " + std::to_string(currentElevationMsl) + " ft";
    std::string elevationAglStr =
        "Elevation (AGL): " + std::to_string(currentElevationAgl) + " ft";
    std::string trueAirspeedStr =
        "True Airspeed: " + std::to_string(trueAirspeed) + " knots";

    std::string verticalVelocityStr;
    if (std::isnan(currentVerticalVelocity)) {
        verticalVelocityStr = "Vertical Velocity: (Error Reading Data)";
    } else {
        verticalVelocityStr = "Vertical Velocity: "
            + std::to_string(currentVerticalVelocity) + " ft/s";
    }
    
    std::string headingPilotStr;
    if (std::isnan(currentPilotHeading)) {
        headingPilotStr = "Error Reading Pilot Heading Data";
    } else {
        headingPilotStr = "Heading, Pilot MagDegrees: " 
            + std::to_string(currentPilotHeading) + " °M";
    }

    std::string headingFlightmodelStr;
    if (std::isnan(currentFlightmodelHeading)) {
        headingFlightmodelStr = "Error Reading Plane Heading Data";
    } else {
        headingFlightmodelStr = "Heading, Flightmodel MagDegrees: "
            + std::to_string(currentFlightmodelHeading) + " °M";
    }
    
    // use this get_next_y_offset() lambda function to find the next vertical pixel start position
    // on the window for string rendering for you.
    int last_offset = 10;
    auto get_next_y_offset = [&last_offset, t]() {
        last_offset = last_offset + 10;
        return t - last_offset;
    };

   // Draw Elevation MSL in window
    XPLMDrawString(
        col_white,
        l + 10,
        get_next_y_offset(),
        elevationMslStr.c_str(),
        NULL,
        xplmFont_Proportional
    );
    // Draw Elevation AGL in window
    XPLMDrawString(
        col_white,
        l + 10,
        get_next_y_offset(),
        elevationAglStr.c_str(),
        NULL,
        xplmFont_Proportional
    );
    // Draw True Airspeed in window
    XPLMDrawString(
        col_white,
        l + 10,
        get_next_y_offset(),
        trueAirspeedStr.c_str(),
        NULL,
        xplmFont_Proportional
    );
    // Draw Vertical Velocity in window
    XPLMDrawString(
        col_white,
        l + 10,
        get_next_y_offset(),
        verticalVelocityStr.c_str(),
        NULL,
        xplmFont_Proportional
    );
    // Draw Pilot Heading in window
    XPLMDrawString(
        col_white, 
        l + 10, 
        get_next_y_offset(),
        headingPilotStr.c_str(), 
        NULL,      
        xplmFont_Proportional
    );
    // Draw Flightmodel Heading in window
    XPLMDrawString(
        col_white, 
        l + 10, 
        get_next_y_offset(),
        headingFlightmodelStr.c_str(), 
        NULL,      
        xplmFont_Proportional
    );
}
