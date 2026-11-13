// This basic plugin is a modified and de-commented version of the sample
// "Hello, World" plugin code found here:
// https://developer.x-plane.com/code-sample/hello-world-sdk-3/

#include <cmath>
#include <cstdio>
#include <ctime>
#include <iostream>
#include <memory>
#include <string>
#include <thread>
#include <vector>

#include "subprojects/baton/lib.rs.h"

using std::string;
using std::vector;

#ifdef __cplusplus
extern "C" {
#endif

#include "XPLMDataAccess.h"
#include "XPLMDisplay.h"
#include "XPLMGraphics.h"
#include "XPLMProcessing.h"
#include "XPLMUtilities.h"
#include <GL/gl.h>

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

// DataRef Identifiers (use lowercase variable names consistently)
// The flightmodel datarefs are in meters and meters per second, so they are
// converted to feet and knots. The pilot datarefs are already in feet and
// knots. Both read_sample() and the window's display need these, so they live
// here: the two halves of a pair must arrive in the same unit (#184).
static const float msToFeetRate = 3.28084f;
static const float msToKnotsRate = 1.94384f;

static XPLMDataRef elevationFlightmodelRef;
static XPLMDataRef elevationPilotRef;
static XPLMDataRef airspeedFlightmodelRef;
static XPLMDataRef airspeedPilotRef;
static XPLMDataRef verticalVelocityFlightmodelRef;
static XPLMDataRef verticalVelocityPilotRef;
static XPLMDataRef headingFlightmodelRef;
static XPLMDataRef headingPilotRef;
// Additional DataRefs for roll/pitch/yaw/g-force (note: no yawPilotRef — yaw
// pilot gauge not available)
static XPLMDataRef yawFlightmodelRef;
static XPLMDataRef rollFlightmodelRef;
static XPLMDataRef rollPilotRef;
static XPLMDataRef pitchFlightmodelRef;
static XPLMDataRef pitchPilotRef;
// g-force now provided as horizontal (side) and vertical (normal) components
static XPLMDataRef gforceHorizontalRef;
static XPLMDataRef gforceVerticalRef;

// baton handle -- a "Box" is the name for a Rust pointer. This Box is handled
// by the cxx crate interface and acts like a normal pointer.
rust::cxxbridge1::Box<Baton> baton = new_baton_handle();

// How often the flight loop sends a sample: every 50ms, 20 Hz
static const int SEND_INTERVAL_MS = 50;

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

static int button_left = 0, button_top = 0, button_right = 0, button_bottom = 0;
static std::string last_send_timestamp = "";

// Helper to get current timestamp as string
std::string get_current_timestamp() {
  std::time_t now = std::time(nullptr);
  char buf[32];
  std::strftime(buf, sizeof(buf), "%Y-%m-%d %H:%M:%S", std::localtime(&now));
  return buf;
}

// Read one sample for the relay: both halves of every value, flightmodel
// first, in the order the relay turns into iMotions entries:
//  [Altitude, Airspeed, VerticalVelocity, Heading, Roll, Pitch, Yaw,
//  G-Force]
//
// Note vertical velocity comes before heading. The old pilot-only payload
// sent heading third and the relay reordered it; the paired payload is in
// entry order instead, so the relay pairs straight through. Sending 16
// values is what lets FlightModel* and Pilot* differ at all (#184).
//
// The pilot datarefs are already in feet and knots; the flightmodel half is
// converted into the same units so the two are comparable. Yaw and G-Force
// have no pilot-side DataRef, so both halves carry the flightmodel value
// (vertical G) and stay equal by design.
vector<float> read_sample() {
  float flightmodelYaw = XPLMGetDataf(yawFlightmodelRef);
  float flightmodelGVert = XPLMGetDataf(gforceVerticalRef);
  return {
      XPLMGetDataf(elevationFlightmodelRef) * msToFeetRate,
      XPLMGetDataf(elevationPilotRef),
      XPLMGetDataf(airspeedFlightmodelRef) * msToKnotsRate,
      XPLMGetDataf(airspeedPilotRef),
      XPLMGetDataf(verticalVelocityFlightmodelRef),
      XPLMGetDataf(verticalVelocityPilotRef),
      XPLMGetDataf(headingFlightmodelRef),
      XPLMGetDataf(headingPilotRef),
      XPLMGetDataf(rollFlightmodelRef),
      XPLMGetDataf(rollPilotRef),
      XPLMGetDataf(pitchFlightmodelRef),
      XPLMGetDataf(pitchPilotRef),
      flightmodelYaw,
      flightmodelYaw,
      flightmodelGVert,
      flightmodelGVert,
  };
}

// Send one sample to the relay and record it in Log.txt. Used by the flight
// loop and by the window's Send Packet button.
void send_sample() {
  vector<float> send_to_baton = read_sample();

  {
    char dbg[320];
    std::snprintf(dbg, sizeof(dbg),
                  "[PDS PLUGIN] Send -> alt=%.3f/%.3f air=%.3f/%.3f "
                  "vv=%.3f/%.3f hdg=%.3f/%.3f (flightmodel/pilot)\n",
                  send_to_baton[0], send_to_baton[1], send_to_baton[2],
                  send_to_baton[3], send_to_baton[4], send_to_baton[5],
                  send_to_baton[6], send_to_baton[7]);
    XPLMDebugString(dbg);
  }
  baton->send(send_to_baton);
}

// Send a sample every SEND_INTERVAL_MS for as long as the plugin is enabled.
// X-Plane runs flight loops whether or not the window is drawn, so data keeps
// flowing with "Positional Flight Data" closed or off-screen (#274). It used
// to be sent from the draw callback, and a window that was never drawn sent
// nothing at all.
float send_flight_loop(float in_elapsed_since_last_call,
                       float in_elapsed_since_last_flight_loop, int in_counter,
                       void *in_refcon) {
  send_sample();

  return SEND_INTERVAL_MS / 1000.0f;
}

// Write one monitor's bounds to Log.txt. Both XPLMGetAllMonitorBoundsGlobal
// and XPLMGetAllMonitorBoundsOS call back with this signature; in_refcon
// names which coordinates the bounds are in.
void log_monitor_bounds(int in_monitor_index, int in_left, int in_top,
                        int in_right, int in_bottom, void *in_refcon) {
  char line[200];
  std::snprintf(
      line, sizeof(line),
      "[PDS PLUGIN] Monitor %d (%s): left=%d top=%d right=%d bottom=%d\n",
      in_monitor_index, static_cast<const char *>(in_refcon), in_left, in_top,
      in_right, in_bottom);
  XPLMDebugString(line);
}

// Update the mouse handler to detect button clicks
int mouse_handler(XPLMWindowID in_window_id, int x, int y, int is_down,
                  void *in_refcon) {
  if (is_down) {
    // Button bounds are set in draw_pilotdatasync_plugin
    if (x >= button_left && x <= button_right && y >= button_bottom &&
        y <= button_top) {
      send_sample();

      last_send_timestamp = get_current_timestamp();
    }
  }
  return 0;
}

//

PLUGIN_API int XPluginStart(char *outName, char *outSig, char *outDesc) {
  strcpy(outName, "PilotDataSyncPlugin");
  strcpy(outSig, "oss.pilotdatasyncplugin");
  strcpy(outDesc, "A plug-in that collects and transmits X-Plane 11 data to "
                  "the iMotions platform for data collection and research");

  XPLMCreateWindow_t params;
  params.structSize = sizeof(params);
  params.visible = 1;
  params.drawWindowFunc = draw_pilotdatasync_plugin;
  params.handleMouseClickFunc = mouse_handler;
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

  // DataRefs for Vertical Velocity
  verticalVelocityFlightmodelRef =
      XPLMFindDataRef("sim/flightmodel/position/vh_ind_fpm");
  verticalVelocityPilotRef =
      XPLMFindDataRef("sim/cockpit2/gauges/indicators/vvi_fpm_pilot");

  // Obtain dataref for Pilot heading and True Magnetic Heading
  headingFlightmodelRef = XPLMFindDataRef("sim/flightmodel/position/mag_psi");
  headingPilotRef = XPLMFindDataRef(
      "sim/cockpit2/gauges/indicators/heading_AHARS_deg_mag_pilot");

  // Obtain datarefs for Yaw, Roll, Pitch
  yawFlightmodelRef = XPLMFindDataRef("sim/flightmodel/position/psi");
  rollFlightmodelRef = XPLMFindDataRef("sim/flightmodel/position/phi");
  rollPilotRef =
      XPLMFindDataRef("sim/cockpit2/gauges/indicators/roll_AHARS_deg_pilot");

  pitchFlightmodelRef = XPLMFindDataRef("sim/flightmodel/position/theta");
  pitchPilotRef =
      XPLMFindDataRef("sim/cockpit2/gauges/indicators/pitch_AHARS_deg_pilot");

  // g-force components (horizontal/side and vertical/normal)
  gforceHorizontalRef = XPLMFindDataRef("sim/flightmodel/forces/g_side");
  gforceVerticalRef = XPLMFindDataRef("sim/flightmodel/forces/g_nrml");

  g_window = XPLMCreateWindowEx(&params);

  // Position the window as a "free" floating window,
  // which the user can drag around
  XPLMSetWindowPositioningMode(g_window, xplm_WindowPositionFree, -1);
  XPLMSetWindowTitle(g_window, "Positional Flight Data");

  // Record where the window ended up, and where the monitors are, so a window
  // nobody can see can be found from Log.txt (#274). The window and the
  // "global" monitor lines share X-Plane's coordinates, but X-Plane lists only
  // the monitors it runs full-screen on there. The "OS" lines list every
  // monitor, in the operating system's pixels.
  int window_left, window_top, window_right, window_bottom;
  XPLMGetWindowGeometry(g_window, &window_left, &window_top, &window_right,
                        &window_bottom);
  char placed[200];
  std::snprintf(placed, sizeof(placed),
                "[PDS PLUGIN] Window placed at left=%d top=%d right=%d "
                "bottom=%d, inside screen bounds left=%d top=%d right=%d "
                "bottom=%d\n",
                window_left, window_top, window_right, window_bottom, left, top,
                right, bottom);
  XPLMDebugString(placed);
  XPLMGetAllMonitorBoundsGlobal(log_monitor_bounds,
                                const_cast<char *>("global"));
  XPLMGetAllMonitorBoundsOS(log_monitor_bounds, const_cast<char *>("OS"));

  return g_window != NULL;
}

PLUGIN_API void XPluginStop() {
  XPLMDestroyWindow(g_window);
  g_window = NULL;
}

PLUGIN_API void XPluginDisable(void) {
  XPLMUnregisterFlightLoopCallback(send_flight_loop, NULL);
  baton->stop();
}

PLUGIN_API int XPluginEnable(void) {
  baton->start();
  XPLMRegisterFlightLoopCallback(send_flight_loop, SEND_INTERVAL_MS / 1000.0f,
                                 NULL);

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

  float currentPilotElevation = XPLMGetDataf(elevationPilotRef);
  string elevationPilotStr =
      build_str("Elevation, Pilot (MSL)", "ft", currentPilotElevation);

  float currentFlightmodelAirspeed =
      XPLMGetDataf(airspeedFlightmodelRef) * msToKnotsRate;
  string airspeedFlightmodelStr =
      build_str("Airspeed, Flightmodel", "knots", currentFlightmodelAirspeed);

  float currentPilotAirspeed = XPLMGetDataf(airspeedPilotRef);
  string airspeedPilotStr =
      build_str("Airspeed, Pilot", "knots", currentPilotAirspeed);

  float currentFlightmodelVerticalVelocity =
      XPLMGetDataf(verticalVelocityFlightmodelRef);
  string verticalVelocityFlightmodelStr =
      build_str("Vertical Velocity, Flightmodel", "ft/min",
                currentFlightmodelVerticalVelocity);

  float currentPilotVerticalVelocity = XPLMGetDataf(verticalVelocityPilotRef);
  string verticalVelocityPilotStr = build_str(
      "Vertical Velocity, Pilot", "ft/min", currentPilotVerticalVelocity);

  float currentFlightmodelHeading = XPLMGetDataf(headingFlightmodelRef);
  string headingFlightmodelStr =
      build_str("Heading, Flightmodel", "°M", currentFlightmodelHeading);

  float currentPilotHeading = XPLMGetDataf(headingPilotRef);
  string headingPilotStr =
      build_str("Heading, Pilot", "°M", currentPilotHeading);

  // New parameters (roll, pitch, yaw, g-force)
  float currentFlightmodelRoll = XPLMGetDataf(rollFlightmodelRef);
  string rollFlightmodelStr =
      build_str("Roll, Flightmodel", "°", currentFlightmodelRoll);
  float currentPilotRoll = XPLMGetDataf(rollPilotRef);
  string rollPilotStr = build_str("Roll, Pilot", "°", currentPilotRoll);

  float currentFlightmodelPitch = XPLMGetDataf(pitchFlightmodelRef);
  string pitchFlightmodelStr =
      build_str("Pitch, Flightmodel", "°", currentFlightmodelPitch);
  float currentPilotPitch = XPLMGetDataf(pitchPilotRef);
  string pitchPilotStr = build_str("Pitch, Pilot", "°", currentPilotPitch);

  float currentFlightmodelYaw = XPLMGetDataf(yawFlightmodelRef);
  // use flightmodel yaw as the pilot yaw value since yaw pilot DataRef is not
  // available
  float currentPilotYaw = currentFlightmodelYaw;
  string yawPilotStr =
      build_str("Yaw, Pilot (from Flightmodel)", "°", currentPilotYaw);
  string yawFlightmodelStr =
      build_str("Yaw, Flightmodel", "°", currentFlightmodelYaw);

  // g-force: show both vertical (normal) and horizontal (side) components.
  float currentFlightmodelGVert = XPLMGetDataf(gforceVerticalRef);
  string gFlightmodelVertStr =
      build_str("G-Force, Flightmodel (Vert)", "G", currentFlightmodelGVert);
  float currentFlightmodelGHoriz = XPLMGetDataf(gforceHorizontalRef);
  string gFlightmodelHorizStr =
      build_str("G-Force, Flightmodel (Horiz)", "G", currentFlightmodelGHoriz);

  // For backward compatibility use vertical G as the single "Pilot G" value
  float currentPilotG = currentFlightmodelGVert;
  string gPilotStr = build_str("G-Force, Pilot (Vert)", "G", currentPilotG);

  // use this get_next_y_offset() lambda function to find the next vertical
  // pixel start position on the window for string rendering for you.
  int last_offset = 10;
  auto get_next_y_offset = [&last_offset, t]() {
    last_offset = last_offset + 10;
    return t - last_offset;
  };

  vector<string> draw_order = {
      elevationFlightmodelStr,
      elevationPilotStr,
      airspeedFlightmodelStr,
      verticalVelocityFlightmodelStr,
      verticalVelocityPilotStr,
      headingFlightmodelStr,
      headingPilotStr,
      rollFlightmodelStr,
      rollPilotStr,
      pitchFlightmodelStr,
      pitchPilotStr,
      yawPilotStr,
      yawPilotStr,
      gFlightmodelVertStr,
      gFlightmodelHorizStr,
      gPilotStr,
  };

  // print each line in order on the window
  for (string line : draw_order) {
    XPLMDrawString(col_white, l + 10, get_next_y_offset(), (char *)line.c_str(),
                   NULL, xplmFont_Proportional);
  }

  // Button dimensions
  int button_width = 120;
  int button_height = 30;
  int button_x = l + 10;
  int button_y = b + 40;

  button_left = button_x;
  button_right = button_x + button_width;
  button_bottom = button_y;
  button_top = button_y + button_height;

  // Draw button rectangle (simple filled rectangle)
  float col_button[] = {0.2f, 0.5f, 0.8f, 1.0f};
  glColor4fv(col_button);
  glBegin(GL_QUADS);
  glVertex2i(button_left, button_bottom);
  glVertex2i(button_right, button_bottom);
  glVertex2i(button_right, button_top);
  glVertex2i(button_left, button_top);
  glEnd();

  // Draw button label
  float col_text[] = {1.0f, 1.0f, 1.0f};
  std::string button_label = "Send Packet";
  XPLMDrawString(col_text, button_left + 10, button_bottom + 8,
                 (char *)button_label.c_str(), NULL, xplmFont_Proportional);

  // Draw timestamp next to button
  std::string ts_label =
      "Last sent: " +
      (last_send_timestamp.empty() ? "Never" : last_send_timestamp);
  XPLMDrawString(col_text, button_right + 10, button_bottom + 8,
                 (char *)ts_label.c_str(), NULL, xplmFont_Proportional);
}
