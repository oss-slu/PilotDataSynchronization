// This basic plugin is a modified and de-commented version of the sample
// "Hello, World" plugin code found here:
// https://developer.x-plane.com/code-sample/hello-world-sdk-3/

#include <cmath>
#include <string>

// #include "packet.cpp"

#ifdef __cplusplus
extern "C" {
#endif

#include "XPLMDataAccess.h"
#include "XPLMDisplay.h"
#include "XPLMGraphics.h"
#include "XPLMProcessing.h"
#include "XPLMUtilities.h"

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
static XPLMFlightLoop_f test;

// DataRef Identifiers
static XPLMDataRef elevationFlightmodelRef;
static XPLMDataRef elevationPilotRef;
static XPLMDataRef airspeedFlightmodelRef;
static XPLMDataRef airspeedPilotRef;
static XPLMDataRef verticalVelocityFlightmodelRef;
static XPLMDataRef verticalVelocityPilotRef;
static XPLMDataRef headingFlightmodelRef;
static XPLMDataRef headingPilotRef;

// Callbacks we will register when we create our window
void draw_pilotdatasync_plugin(XPLMWindowID in_window_id, void* in_refcon);

float flight_loop(
    float inElapsedSinceLastCall,
    float inElapsedTimeSinceLastFlightLoop,
    int inCounter,
    void* inRefcon
) {
    return 1.0;
}

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
    strcpy(
        outDesc,
        "A plug-in that collects and transmits X-Plane 11 data to the iMotions platform for data collection and research"
    );

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
    elevationFlightmodelRef =
        XPLMFindDataRef("sim/flightmodel/position/elevation");
    elevationPilotRef =
        XPLMFindDataRef("sim/cockpit2/gauges/indicators/altitude_ft_pilot");

    // Obtain datarefs for Airspeed
    airspeedFlightmodelRef =
        XPLMFindDataRef("sim/flightmodel/position/true_airspeed");
    airspeedPilotRef =
        XPLMFindDataRef("sim/cockpit2/gauges/indicators/airspeed_kts_pilot");

    // DataRefs for Vertical Velocitys
    verticalVelocityFlightmodelRef =
        XPLMFindDataRef("sim/flightmodel/position/vh_ind_fpm");
    verticalVelocityPilotRef =
        XPLMFindDataRef("sim/cockpit2/gauges/indicators/vvi_fpm_pilot");

    // Obtain dataref for Pilot heading and True Magnetic Heading
    headingFlightmodelRef = XPLMFindDataRef("sim/flightmodel/position/mag_psi");
    headingPilotRef = XPLMFindDataRef(
        "sim/cockpit2/gauges/indicators/heading_AHARS_deg_mag_pilot"
    );

    g_window = XPLMCreateWindowEx(&params);

    // Position the window as a "free" floating window,
    // which the user can drag around
    XPLMSetWindowPositioningMode(g_window, xplm_WindowPositionFree, -1);
    XPLMSetWindowTitle(g_window, "Positional Flight Data");

    // Register per-time-unit callback
    XPLMCreateFlightLoop_t loop_params = {
        .structSize = sizeof(loop_params),
        .phase = xplm_FlightLoop_Phase_BeforeFlightModel,
        .callbackFunc = flight_loop,olo
        .refcon = NULL  
    };
    XPLMFlightLoopID id = XPLMCreateFlightLoop(&loop_params);
    XPLMScheduleFlightLoop(id, 1.0, true);

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

    // Dataref provides altitudes in meters, need to convert to feet and knots
    float msToFeetRate = 3.28084;
    float msToKnotsRate = 1.94384;

    // Create strings from DataRefs to display in plugin window
    std::string elevationFlightmodelStr;
    float currentFlightmodelElevation =
        XPLMGetDataf(elevationFlightmodelRef) * msToFeetRate;
    if (std::isnan(currentFlightmodelElevation)) {
        elevationFlightmodelStr =
            "Elevation, Flightmodel (MSL): (Error Reading Data)";
    } else {
        elevationFlightmodelStr = "Elevation, Flightmodel (MSL):"
            + std::to_string(currentFlightmodelElevation) + " ft";
    }

    std::string elevationPilotStr;
    float currentPilotElevation =
        XPLMGetDataf(elevationPilotRef) * msToFeetRate;
    if (std::isnan(currentPilotElevation)) {
        elevationPilotStr = "Elevation, Pilot (MSL): (Error Reading Data)";
    } else {
        elevationPilotStr = "Elevation, Pilot (MSL):"
            + std::to_string(currentPilotElevation) + " ft";
    }

    std::string airspeedFlightmodelStr;
    float currentFlightmodelAirspeed =
        XPLMGetDataf(airspeedFlightmodelRef) * msToKnotsRate;
    if (std::isnan(currentFlightmodelAirspeed)) {
        airspeedFlightmodelStr = "Airspeed, Flightmodel: (Error Reading Data)";
    } else {
        airspeedFlightmodelStr = "Airspeed, Flightmodel:"
            + std::to_string(currentFlightmodelAirspeed) + " knots";
    }

    std::string airspeedPilotStr;
    float currentPilotAirspeed = XPLMGetDataf(airspeedPilotRef) * msToKnotsRate;
    if (std::isnan(currentPilotAirspeed)) {
        airspeedPilotStr = "Airspeed, Pilot: (Error Reading Data)";
    } else {
        airspeedPilotStr = "Airspeed, Pilot:"
            + std::to_string(currentPilotAirspeed) + " knots";
    }

    std::string verticalVelocityFlightmodelStr;
    float currentFlightmodelVerticalVelocity =
        XPLMGetDataf(verticalVelocityFlightmodelRef);
    if (std::isnan(currentFlightmodelVerticalVelocity)) {
        verticalVelocityFlightmodelStr =
            "Vertical Velocity, Flightmodel: (Error Reading Data)";
    } else {
        verticalVelocityFlightmodelStr = "Vertical Velocity, Flightmodel: "
            + std::to_string(currentFlightmodelVerticalVelocity) + " ft/min";
    }

    std::string verticalVelocityPilotStr;
    float currentPilotVerticalVelocity = XPLMGetDataf(verticalVelocityPilotRef);
    if (std::isnan(currentPilotVerticalVelocity)) {
        verticalVelocityPilotStr =
            "Vertical Velocity, Flightmodel: (Error Reading Data)";
    } else {
        verticalVelocityPilotStr = "Vertical Velocity, Pilot: "
            + std::to_string(currentPilotVerticalVelocity) + " ft/min";
    }

    std::string headingFlightmodelStr;
    float currentFlightmodelHeading = XPLMGetDataf(headingFlightmodelRef);
    if (std::isnan(currentFlightmodelHeading)) {
        headingFlightmodelStr = "Heading, Flightmodel: (Error Reading Data)";
    } else {
        headingFlightmodelStr = "Heading, Flightmodel: "
            + std::to_string(currentFlightmodelHeading) + " °M";
    }

    std::string headingPilotStr;
    float currentPilotHeading = XPLMGetDataf(headingPilotRef);
    if (std::isnan(currentPilotHeading)) {
        headingPilotStr = "Heading, Pilot: (Error Reading Data)";
    } else {
        headingPilotStr =
            "Heading, Pilot: " + std::to_string(currentPilotHeading) + " °M";
    }

    // use this get_next_y_offset() lambda function to find the next vertical pixel start position
    // on the window for string rendering for you.
    int last_offset = 10;
    auto get_next_y_offset = [&last_offset, t]() {
        last_offset = last_offset + 10;
        return t - last_offset;
    };

    // Draw Elevation in window
    XPLMDrawString(
        col_white,
        l + 10,
        get_next_y_offset(),
        elevationFlightmodelStr.c_str(),
        NULL,
        xplmFont_Proportional
    );
    XPLMDrawString(
        col_white,
        l + 10,
        get_next_y_offset(),
        elevationPilotStr.c_str(),
        NULL,
        xplmFont_Proportional
    );
    // Draw Airspeed in window
    XPLMDrawString(
        col_white,
        l + 10,
        get_next_y_offset(),
        airspeedFlightmodelStr.c_str(),
        NULL,
        xplmFont_Proportional
    );
    XPLMDrawString(
        col_white,
        l + 10,
        get_next_y_offset(),
        airspeedPilotStr.c_str(),
        NULL,
        xplmFont_Proportional
    );
    // Draw Vertical Velocity in window
    XPLMDrawString(
        col_white,
        l + 10,
        get_next_y_offset(),
        verticalVelocityFlightmodelStr.c_str(),
        NULL,
        xplmFont_Proportional
    );
    XPLMDrawString(
        col_white,
        l + 10,
        get_next_y_offset(),
        verticalVelocityPilotStr.c_str(),
        NULL,
        xplmFont_Proportional
    );
    // Draw Heading in window
    XPLMDrawString(
        col_white,
        l + 10,
        get_next_y_offset(),
        headingFlightmodelStr.c_str(),
        NULL,
        xplmFont_Proportional
    );
    XPLMDrawString(
        col_white,
        l + 10,
        get_next_y_offset(),
        headingPilotStr.c_str(),
        NULL,
        xplmFont_Proportional
    );
}
