// This basic plugin is a modified and de-commented version of the sample
// "Hello, World" plugin code found here:
// https://developer.x-plane.com/code-sample/hello-world-sdk-3/

#include <cmath>
#include <iostream>
#include <memory>
#include <string>
#include <thread>
#include <vector>

#include "Logger.cpp"
#include "TCPClient.cpp"
#include "subprojects/baton/lib.rs.h"
#include "threading-tools.h"

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

// An opaque handle to the window we will create
static XPLMWindowID g_window;

// DataRef Identifiers
static XPLMDataRef elevationFlightmodelRef;
static XPLMDataRef elevationPilotRef;
static XPLMDataRef airspeedFlightmodelRef;
static XPLMDataRef airspeedPilotRef;
static XPLMDataRef verticalVelocityFlightmodelRef;
static XPLMDataRef verticalVelocityPilotRef;
static XPLMDataRef headingFlightmodelRef;
static XPLMDataRef headingPilotRef;

// thread handle for TCP
static std::thread thread_handle;

// baton handle
rust::cxxbridge1::Box<ThreadWrapper> baton = new_wrapper();

// Callbacks we will register when we create our window
void draw_pilotdatasync_plugin(XPLMWindowID in_window_id, void *in_refcon);

int dummy_mouse_handler(XPLMWindowID in_window_id, int x, int y, int is_down,
                        void *in_refcon) {
  return 0;
}

XPLMCursorStatus dummy_cursor_status_handler(XPLMWindowID in_window_id, int x,
                                             int y, void *in_refcon) {
  return xplm_CursorDefault;
}

int dummy_wheel_handler(XPLMWindowID in_window_id, int x, int y, int wheel,
                        int clicks, void *in_refcon) {
  return 0;
}

void dummy_key_handler(XPLMWindowID in_window_id, char key, XPLMKeyFlags flags,
                       char virtual_key, void *in_refcon, int losing_focus) {}

volatile bool stop_exec = false;
Logger *Logger::instance = nullptr;

int runTCP(std::shared_ptr<ThreadQueue> thread_queue) {
  Logger *logger = Logger::getInstance();

  logger->log("Plugin initialization started");

  try {
    TCPClient client;
    std::string serverIP = "127.0.0.1";
    logger->log("Initializing Winsock");
    client.initializeWinsock();

    logger->log("Creating socket");
    client.createSocket();

    logger->log("Attempting to connect to server");
    if (!client.connectToServer(serverIP)) {
      logger->log("Server connection failed", MsgLogType::CONN_FAIL);
      // Handle connection failure
      return 0;
    }

    logger->log("Server connection successful", MsgLogType::CONN_PASS);

    bool stop_exec = false;

    while (!stop_exec) {
      if (thread_queue->size() == 0) {
        std::this_thread::sleep_for(std::chrono::milliseconds(150));
        continue;
      }

      ThreadMessage tm = thread_queue->pop();
      if (tm.end_execution_flag) {
        logger->log("Received end execution flag", MsgLogType::END);
        stop_exec = true;
      } else {
        std::vector<std::string> myVec;
        for (int i = 0; i < 4; i++) {
          myVec.push_back(std::to_string(tm.values_for_packet[i]));
        }

        std::string packet = generate_packet(myVec);
        logger->log("Generating packet: " + packet);

        if (!client.sendData(packet)) {
          logger->log("Failed to send packet", MsgLogType::SEND_FAIL);
        } else {
          logger->log("Packet sent successfully", MsgLogType::SEND_PASS);
        }
      }
    }
  } catch (const std::exception &e) {
    logger->log("Exception occurred: " + std::string(e.what()),
                MsgLogType::ERR);
    return 0;
  }

  return 1;
}

PLUGIN_API int XPluginStart(char *outName, char *outSig, char *outDesc) {
  strcpy(outName, "PilotDataSyncPlugin");
  strcpy(outSig, "oss.pilotdatasyncplugin");
  strcpy(outDesc, "A plug-in that collects and transmits X-Plane 11 data to "
                  "the iMotions platform for data collection and research");

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
  params.right = params.left + 350;
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
      "sim/cockpit2/gauges/indicators/heading_AHARS_deg_mag_pilot");

  g_window = XPLMCreateWindowEx(&params);

  // Position the window as a "free" floating window,
  // which the user can drag around
  XPLMSetWindowPositioningMode(g_window, xplm_WindowPositionFree, -1);
  XPLMSetWindowTitle(g_window, "Positional Flight Data");

  return g_window != NULL;
}

PLUGIN_API void XPluginStop() {
  XPLMDestroyWindow(g_window);
  g_window = NULL;
}

PLUGIN_API void XPluginDisable(void) { baton->stop(); }

PLUGIN_API int XPluginEnable(void) {
  baton->start();
  return 1;
}

PLUGIN_API void XPluginReceiveMessage(XPLMPluginID inFrom, int inMsg,
                                      void *inParam) {}

void draw_pilotdatasync_plugin(XPLMWindowID in_window_id, void *in_refcon) {
  XPLMSetGraphicsState(0 /* no fog */, 0 /* 0 texture units */,
                       0 /* no lighting */, 0 /* no alpha testing */,
                       1 /* do alpha blend */, 1 /* do depth testing */,
                       0 /* no depth writing */
  );

  int l, t, r, b;
  XPLMGetWindowGeometry(in_window_id, &l, &t, &r, &b);

  float col_white[] = {1.0, 1.0, 1.0}; // RGB

  // Dataref provides altitudes in meters, need to convert to feet and knots
  float msToFeetRate = 3.28084;
  float msToKnotsRate = 1.94384;

  auto build_str = [](string label, string unit, float value) {
    string suffix = !std::isnan(value) ? std::to_string(value) + " " + unit
                                       : "(Error Reading Data)";
    return label + ": " + suffix;
  };

  // Create strings from DataRefs to display in plugin window
  float currentFlightmodelElevation =
      XPLMGetDataf(elevationFlightmodelRef) * msToFeetRate;
  string elevationFlightmodelStr = build_str("Elevation, Flightmodel (MSL)",
                                             "ft", currentFlightmodelElevation);

  float currentPilotElevation = XPLMGetDataf(elevationPilotRef) * msToFeetRate;
  string elevationPilotStr =
      build_str("Elevation, Pilot (MSL)", "ft", currentPilotElevation);

  float currentFlightmodelAirspeed =
      XPLMGetDataf(airspeedFlightmodelRef) * msToKnotsRate;
  string airspeedFlightmodelStr =
      build_str("Airspeed, Flightmodel", "knots", currentFlightmodelAirspeed);

  float currentPilotAirspeed = XPLMGetDataf(airspeedPilotRef) * msToKnotsRate;
  string airspeedPilotStr =
      build_str("Airspeed, Pilot", "knots", currentPilotAirspeed);

  float currentFlightmodelVerticalVelocity =
      XPLMGetDataf(verticalVelocityFlightmodelRef);
  string verticalVelocityFlightmodelStr =
      build_str("Vertical Velocity, Flightmodel", "ft/min",
                currentFlightmodelVerticalVelocity);

  float currentPilotVerticalVelocity = XPLMGetDataf(verticalVelocityPilotRef);
  string verticalVelocityPilotStr = build_str(
      "Vertical Velocity, Flightmodel", "ft/min", currentPilotVerticalVelocity);

  float currentFlightmodelHeading = XPLMGetDataf(headingFlightmodelRef);
  string headingFlightmodelStr =
      build_str("Heading, Flightmodel", "°M", currentFlightmodelHeading);

  float currentPilotHeading = XPLMGetDataf(headingPilotRef);
  string headingPilotStr =
      build_str("Heading, Pilot", "°M", currentPilotHeading);

  // use this get_next_y_offset() lambda function to find the next vertical
  // pixel start position on the window for string rendering for you.
  int last_offset = 10;
  auto get_next_y_offset = [&last_offset, t]() {
    last_offset = last_offset + 10;
    return t - last_offset;
  };

  vector<string> draw_order = {
      elevationFlightmodelStr,  elevationPilotStr,
      airspeedFlightmodelStr,   verticalVelocityFlightmodelStr,
      verticalVelocityPilotStr, headingFlightmodelStr,
      headingPilotStr,
  };

  // print each line in order on the window
  for (string line : draw_order) {
    XPLMDrawString(col_white, l + 10, get_next_y_offset(), (char *)line.c_str(),
                   NULL, xplmFont_Proportional);
  }

  // Send flight data to Relay via Baton. Order may be incorrect, and iMotions'
  // .xml file must be modified to reflect any changes here
  vector<float> send_to_baton = {
      currentPilotElevation,
      currentPilotAirspeed,
      currentPilotHeading,
      currentPilotVerticalVelocity,
  };
  baton->send(send_to_baton);
}