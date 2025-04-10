// This basic plugin is a modified and de-commented version of the sample
// "Hello, World" plugin code found here:
// https://developer.x-plane.com/code-sample/hello-world-sdk-3/

#include <cmath>
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

float flight_loop(float inElapsedSinceLastCall,
                  float inElapsedTimeSinceLastFlightLoop, int inCounter,
                  void *inRefcon) {
  // currently sends a series of test packets. once the server is stable, we
  // will send the real data
  /* ThreadQueue *tq_ptr = (ThreadQueue*)inRefcon;
for (int i = 1; i < 4; i++) {
  ThreadMessage tm = { {(float)i, (float)i, (float)i, (float)i}, false };
  tq_ptr->push(tm);
}
ThreadMessage tm = { {}, true};
tq_ptr->push(tm);

// return 0.0 to deactivate the loop. otherwise, return val == number of secs
until next callback return 0.0; */

  ThreadQueue *tq_ptr = (ThreadQueue *)inRefcon;

  // Retrieve X-Plane datarefs
  float altitude =
      XPLMGetDataf(elevationFlightmodelRef); // Altitude above sea level
  float groundSpeed = XPLMGetDataf(airspeedFlightmodelRef); // Ground speed
  float heading = XPLMGetDataf(headingFlightmodelRef);      // Magnetic heading
  float verticalSpeed =
      XPLMGetDataf(verticalVelocityFlightmodelRef); // Vertical speed

  std::vector<std::string> dataVector = {
      std::to_string(altitude), std::to_string(groundSpeed),
      std::to_string(heading), std::to_string(verticalSpeed)};

  // Create thread message with the data vector
  ThreadMessage tm = {{altitude, groundSpeed, heading, verticalSpeed}, false};
  tq_ptr->push(tm);

  // Send end execution message
  /*  ThreadMessage end_tm = { {}, true};
tq_ptr->push(end_tm); */

  // Return 1.0 to call again in 1 second
  return 1.0;
}

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
  stop_exec = true;
  if (thread_handle.joinable()) {
    thread_handle.join(); // Wait for the thread to finish
  }
  XPLMDestroyWindow(g_window);
  g_window = NULL;
}

PLUGIN_API void XPluginDisable(void) { baton->stop(); }

PLUGIN_API int XPluginEnable(void) {
  // TEMPORARILY DISABLED PENDING REPLACEMENT VIA BATON

  /*
// TCP server threading setup
ThreadQueue tq;
std::shared_ptr<ThreadQueue> tq_ptr = std::make_shared<ThreadQueue>();
thread_handle = std::thread(runTCP, tq_ptr);

// Register per-time-unit callback
XPLMCreateFlightLoop_t loop_params = {
  .structSize = sizeof(loop_params),
  .phase = xplm_FlightLoop_Phase_BeforeFlightModel,
  .callbackFunc = flight_loop,
  .refcon = tq_ptr.get()
};
XPLMFlightLoopID id = XPLMCreateFlightLoop(&loop_params);
XPLMScheduleFlightLoop(id, 1.0, true);
*/

  // baton test
  // auto thread_wrapper = new_wrapper();
  // thread_wrapper->start();
  // thread_wrapper->stop();
  // baton = new_wrapper();
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

  // Create strings from DataRefs to display in plugin window
  std::string elevationFlightmodelStr;
  float currentFlightmodelElevation =
      XPLMGetDataf(elevationFlightmodelRef) * msToFeetRate;
  if (std::isnan(currentFlightmodelElevation)) {
    elevationFlightmodelStr =
        "Elevation, Flightmodel (MSL): (Error Reading Data)";
  } else {
    elevationFlightmodelStr = "Elevation, Flightmodel (MSL):" +
                              std::to_string(currentFlightmodelElevation) +
                              " ft";
  }

  std::string elevationPilotStr;
  float currentPilotElevation = XPLMGetDataf(elevationPilotRef) * msToFeetRate;
  if (std::isnan(currentPilotElevation)) {
    elevationPilotStr = "Elevation, Pilot (MSL): (Error Reading Data)";
  } else {
    elevationPilotStr =
        "Elevation, Pilot (MSL):" + std::to_string(currentPilotElevation) +
        " ft";
  }

  std::string airspeedFlightmodelStr;
  float currentFlightmodelAirspeed =
      XPLMGetDataf(airspeedFlightmodelRef) * msToKnotsRate;
  if (std::isnan(currentFlightmodelAirspeed)) {
    airspeedFlightmodelStr = "Airspeed, Flightmodel: (Error Reading Data)";
  } else {
    airspeedFlightmodelStr =
        "Airspeed, Flightmodel:" + std::to_string(currentFlightmodelAirspeed) +
        " knots";
  }

  std::string airspeedPilotStr;
  float currentPilotAirspeed = XPLMGetDataf(airspeedPilotRef) * msToKnotsRate;
  if (std::isnan(currentPilotAirspeed)) {
    airspeedPilotStr = "Airspeed, Pilot: (Error Reading Data)";
  } else {
    airspeedPilotStr =
        "Airspeed, Pilot:" + std::to_string(currentPilotAirspeed) + " knots";
  }

  std::string verticalVelocityFlightmodelStr;
  float currentFlightmodelVerticalVelocity =
      XPLMGetDataf(verticalVelocityFlightmodelRef);
  if (std::isnan(currentFlightmodelVerticalVelocity)) {
    verticalVelocityFlightmodelStr =
        "Vertical Velocity, Flightmodel: (Error Reading Data)";
  } else {
    verticalVelocityFlightmodelStr =
        "Vertical Velocity, Flightmodel: " +
        std::to_string(currentFlightmodelVerticalVelocity) + " ft/min";
  }

  std::string verticalVelocityPilotStr;
  float currentPilotVerticalVelocity = XPLMGetDataf(verticalVelocityPilotRef);
  if (std::isnan(currentPilotVerticalVelocity)) {
    verticalVelocityPilotStr =
        "Vertical Velocity, Flightmodel: (Error Reading Data)";
  } else {
    verticalVelocityPilotStr = "Vertical Velocity, Pilot: " +
                               std::to_string(currentPilotVerticalVelocity) +
                               " ft/min";
  }

  std::string headingFlightmodelStr;
  float currentFlightmodelHeading = XPLMGetDataf(headingFlightmodelRef);
  if (std::isnan(currentFlightmodelHeading)) {
    headingFlightmodelStr = "Heading, Flightmodel: (Error Reading Data)";
  } else {
    headingFlightmodelStr =
        "Heading, Flightmodel: " + std::to_string(currentFlightmodelHeading) +
        " °M";
  }

  std::string headingPilotStr;
  float currentPilotHeading = XPLMGetDataf(headingPilotRef);
  if (std::isnan(currentPilotHeading)) {
    headingPilotStr = "Heading, Pilot: (Error Reading Data)";
  } else {
    headingPilotStr =
        "Heading, Pilot: " + std::to_string(currentPilotHeading) + " °M";
  }

  // use this get_next_y_offset() lambda function to find the next vertical
  // pixel start position on the window for string rendering for you.
  int last_offset = 10;
  auto get_next_y_offset = [&last_offset, t]() {
    last_offset = last_offset + 10;
    return t - last_offset;
  };

  // Draw Elevation in window
  XPLMDrawString(col_white, l + 10, get_next_y_offset(),
                 elevationFlightmodelStr.c_str(), NULL, xplmFont_Proportional);
  XPLMDrawString(col_white, l + 10, get_next_y_offset(),
                 elevationPilotStr.c_str(), NULL, xplmFont_Proportional);
  // Draw Airspeed in window
  XPLMDrawString(col_white, l + 10, get_next_y_offset(),
                 airspeedFlightmodelStr.c_str(), NULL, xplmFont_Proportional);
  XPLMDrawString(col_white, l + 10, get_next_y_offset(),
                 airspeedPilotStr.c_str(), NULL, xplmFont_Proportional);
  // Draw Vertical Velocity in window
  XPLMDrawString(col_white, l + 10, get_next_y_offset(),
                 verticalVelocityFlightmodelStr.c_str(), NULL,
                 xplmFont_Proportional);
  XPLMDrawString(col_white, l + 10, get_next_y_offset(),
                 verticalVelocityPilotStr.c_str(), NULL, xplmFont_Proportional);
  // Draw Heading in window
  XPLMDrawString(col_white, l + 10, get_next_y_offset(),
                 headingFlightmodelStr.c_str(), NULL, xplmFont_Proportional);
  XPLMDrawString(col_white, l + 10, get_next_y_offset(),
                 headingPilotStr.c_str(), NULL, xplmFont_Proportional);

  Logger *instance = Logger::getInstance();
  if (instance == nullptr) {
    return;
  }

  // blank line
  XPLMDrawString(col_white, l + 10, get_next_y_offset(), string("").c_str(),
                 NULL, xplmFont_Proportional);

  string dashboard_header = "Packet Data:";
  XPLMDrawString(col_white, l + 10, get_next_y_offset(),
                 dashboard_header.c_str(), NULL, xplmFont_Proportional);

  MsgLogType status = instance->get_last_status();
  string status_str;
  switch (status) {
  case MsgLogType::NONE:
    status_str = "NONE";
    break;
  case MsgLogType::SEND:
    status_str = "SEND";
    break;
  case MsgLogType::SEND_PASS:
    status_str = "SEND_PASS";
    break;
  case MsgLogType::SEND_FAIL:
    status_str = "SEND_FAIL";
    break;
  case MsgLogType::CONN:
    status_str = "CONN";
    break;
  case MsgLogType::CONN_PASS:
    status_str = "CONN_PASS";
    break;
  case MsgLogType::CONN_FAIL:
    status_str = "CONN_FAIL";
    break;
  case MsgLogType::END:
    status_str = "END";
    break;
  case MsgLogType::ERR:
    status_str = "ERR";
    break;
  default:
    status_str = "N/A";
    break;
  };

  XPLMDrawString(col_white, l + 10, get_next_y_offset(),
                 string("LAST STATUS: " + status_str).c_str(), NULL,
                 xplmFont_Proportional);

  string packets_sent =
      "Packets Sent: " + to_string(instance->get_packets_sent());
  XPLMDrawString(col_white, l + 10, get_next_y_offset(), packets_sent.c_str(),
                 NULL, xplmFont_Proportional);

  // BATON TEST
  baton->send(currentPilotElevation);
  //
}