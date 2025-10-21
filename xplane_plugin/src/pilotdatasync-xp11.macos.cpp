// This is the mac version of the same .cpp file for xplane 11

#include <cmath>
#include <ctime>
#include <iostream>
#include <memory>
#include <string>
#include <thread>
#include <vector>

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
#ifdef APL
#include <OpenGL/gl.h>
#else
#include <GL/gl.h>
#endif

#ifdef __cplusplus
}
#endif
// These were added for UDP functionality -Nyla Hughes
#include <arpa/inet.h>
#include <cstring>
#include <netinet/in.h>
#include <sys/socket.h>
#include <unistd.h>

#ifndef XPLM300
#error This is made to be compiled against the XPLM300 SDK
#endif

static XPLMWindowID g_window;

static XPLMDataRef elevationFlightmodelRef;
static XPLMDataRef elevationPilotRef;
static XPLMDataRef airspeedFlightmodelRef;
static XPLMDataRef airspeedPilotRef;
static XPLMDataRef verticalVelocityFlightmodelRef;
static XPLMDataRef verticalVelocityPilotRef;
static XPLMDataRef headingFlightmodelRef;
static XPLMDataRef headingPilotRef;

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
static std::time_t g_last_udp_sent =
    0; // added to track last UDP sent time - Nyla Hughes

// these were added to store and manage the UDP socket - Nyla Hughes
static int g_udp_socket = -1;
static struct sockaddr_in g_udp_addr;

std::string get_current_timestamp() {
  std::time_t now = std::time(nullptr);
  char buf[32];
  std::strftime(buf, sizeof(buf), "%Y-%m-%d %H:%M:%S", std::localtime(&now));
  return buf;
}

// this was added to create and initialize the UDP socket - Nyla Hughes
static void udp_init(const char *ip, int port) {
  g_udp_socket = socket(AF_INET, SOCK_DGRAM, 0);
  if (g_udp_socket < 0) {
    XPLMDebugString("[PilotDataSync] UDP socket create FAILED\n");
    return;
  }
  std::memset(&g_udp_addr, 0, sizeof(g_udp_addr));
  g_udp_addr.sin_family = AF_INET;
  g_udp_addr.sin_port = htons(port);
  inet_pton(AF_INET, ip, &g_udp_addr.sin_addr);
}

// added to send UDP packets - Nyla Hughes
static void udp_send(const std::string &payload) {
  if (g_udp_socket < 0)
    return;
  sendto(g_udp_socket, payload.c_str(), (int)payload.size(), 0,
         (struct sockaddr *)&g_udp_addr, sizeof(g_udp_addr));
}

int mouse_handler(XPLMWindowID in_window_id, int x, int y, int is_down,
                  void *in_refcon) {
  if (is_down) {
    if (x >= button_left && x <= button_right && y >= button_bottom &&
        y <= button_top) {
      float msToFeetRate = 3.28084;
      float msToKnotsRate = 1.94384;

      float currentPilotElevation = XPLMGetDataf(
          elevationPilotRef); // added to get pilot elevation - Nyla Hughes
      float currentPilotAirspeed = XPLMGetDataf(
          airspeedPilotRef); // added to get pilot airspeed - Nyla Hughes
      float currentPilotHeading = XPLMGetDataf(headingPilotRef);
      float currentPilotVerticalVelocity =
          XPLMGetDataf(verticalVelocityPilotRef);

      std::vector<float> send_to_baton = {
          currentPilotElevation,
          currentPilotAirspeed,
          currentPilotHeading,
          currentPilotVerticalVelocity,
      };

      last_send_timestamp = get_current_timestamp();

      // added to send packets with the send packet button - Nyla Hughes
      char clickPkt[256];
      std::snprintf(
          clickPkt, sizeof(clickPkt),
          "Packet button clicked Altitude: %.5f ft | Airspeed: %.5f knots | "
          "Vertical Speed: %.5f ft/min | Heading: %.5f deg M | \n",
          currentPilotElevation, currentPilotAirspeed,
          currentPilotVerticalVelocity, currentPilotHeading);
      udp_send(std::string(clickPkt));
      XPLMDebugString(
          (std::string("[PilotDataSync] ") + clickPkt + "\n").c_str());
    }
  }
  return 0;
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
  params.handleMouseClickFunc = mouse_handler;
  params.handleRightClickFunc = dummy_mouse_handler;
  params.handleMouseWheelFunc = dummy_wheel_handler;
  params.handleKeyFunc = dummy_key_handler;
  params.handleCursorFunc = dummy_cursor_status_handler;
  params.refcon = NULL;
  params.layer = xplm_WindowLayerFloatingWindows;

  int left, bottom, right, top;
  XPLMGetScreenBoundsGlobal(&left, &top, &right, &bottom);
  params.left = left + 50;
  params.bottom = bottom + 150;
  params.right = params.left + 350;
  params.top = params.bottom + 200;

  elevationFlightmodelRef =
      XPLMFindDataRef("sim/flightmodel/position/elevation");
  elevationPilotRef =
      XPLMFindDataRef("sim/cockpit2/gauges/indicators/altitude_ft_pilot");
  airspeedFlightmodelRef =
      XPLMFindDataRef("sim/flightmodel/position/true_airspeed");
  airspeedPilotRef =
      XPLMFindDataRef("sim/cockpit2/gauges/indicators/airspeed_kts_pilot");
  verticalVelocityFlightmodelRef =
      XPLMFindDataRef("sim/flightmodel/position/vh_ind_fpm");
  verticalVelocityPilotRef =
      XPLMFindDataRef("sim/cockpit2/gauges/indicators/vvi_fpm_pilot");
  headingFlightmodelRef = XPLMFindDataRef("sim/flightmodel/position/mag_psi");
  headingPilotRef = XPLMFindDataRef(
      "sim/cockpit2/gauges/indicators/heading_AHARS_deg_mag_pilot");

  g_window = XPLMCreateWindowEx(&params);
  XPLMSetWindowPositioningMode(g_window, xplm_WindowPositionFree, -1);
  XPLMSetWindowTitle(g_window, "Positional Flight Data");

  udp_init("127.0.0.1", 49005); // this is the ip for my local machine but idk
                                // if this will work for everyone
  // I want to try and make this a configurable option later - Nyla Hughes

  return g_window != NULL;
}

PLUGIN_API void XPluginStop() {
  // added this to check if the udp socket is open before closing it - Nyla
  // Hughes
  if (g_udp_socket >= 0) {
    close(g_udp_socket);
    g_udp_socket = -1;
  }

  XPLMDestroyWindow(g_window);
  g_window = NULL;
}

PLUGIN_API int XPluginEnable(void) { return 1; }

PLUGIN_API void XPluginReceiveMessage(XPLMPluginID inFrom, int inMsg,
                                      void *inParam) {}

void draw_pilotdatasync_plugin(XPLMWindowID in_window_id, void *in_refcon) {
  XPLMSetGraphicsState(0, 0, 0, 0, 1, 1, 0);

  int l, t, r, b;
  XPLMGetWindowGeometry(in_window_id, &l, &t, &r, &b);
  float col_white[] = {1.0, 1.0, 1.0};

  float msToFeetRate = 3.28084;
  float msToKnotsRate = 1.94384;

  auto build_str = [](string label, string unit, float value) {
    string suffix = !std::isnan(value) ? std::to_string(value) + " " + unit
                                       : "(Error Reading Data)";
    return label + ": " + suffix;
  };

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
      "Vertical Velocity, Flightmodel", "ft/min", currentPilotVerticalVelocity);

  float currentFlightmodelHeading = XPLMGetDataf(headingFlightmodelRef);
  string headingFlightmodelStr =
      build_str("Heading, Flightmodel", "°M", currentFlightmodelHeading);

  float currentPilotHeading = XPLMGetDataf(headingPilotRef);
  string headingPilotStr =
      build_str("Heading, Pilot", "°M", currentPilotHeading);

  // added to send UDP packets every second - Nyla Hughes
  std::time_t now_sec = std::time(nullptr);
  if (now_sec != g_last_udp_sent) {
    char pkt[256];
    std::snprintf(pkt, sizeof(pkt),
                  "Altitude: %.5f ft | Airspeed: %.5f knots | Vertical Speed: "
                  "%.5f ft/min | Heading: %.5f deg M | \n",
                  currentPilotElevation, currentPilotAirspeed,
                  currentPilotVerticalVelocity, currentPilotHeading);

    udp_send(std::string(pkt));
    XPLMDebugString((std::string("[PilotDataSync] ") + pkt + "\n").c_str());
    g_last_udp_sent = now_sec;
  }

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

  for (string line : draw_order) {
    XPLMDrawString(col_white, l + 10, get_next_y_offset(), (char *)line.c_str(),
                   NULL, xplmFont_Proportional);
  }

  int button_width = 120;
  int button_height = 30;
  int button_x = l + 10;
  int button_y = b + 40;

  button_left = button_x;
  button_right = button_x + button_width;
  button_bottom = button_y;
  button_top = button_y + button_height;

  float col_button[] = {0.2f, 0.5f, 0.8f, 1.0f};
  glColor4fv(col_button);
  glBegin(GL_QUADS);
  glVertex2i(button_left, button_bottom);
  glVertex2i(button_right, button_bottom);
  glVertex2i(button_right, button_top);
  glVertex2i(button_left, button_top);
  glEnd();

  float col_text[] = {1.0f, 1.0f, 1.0f};
  std::string button_label = "Send Packet";
  XPLMDrawString(col_text, button_left + 10, button_bottom + 8,
                 (char *)button_label.c_str(), NULL, xplmFont_Proportional);

  std::string ts_label =
      "Last sent: " +
      (last_send_timestamp.empty() ? "Never" : last_send_timestamp);
  XPLMDrawString(col_text, button_right + 10, button_bottom + 8,
                 (char *)ts_label.c_str(), NULL, xplmFont_Proportional);

  vector<float> send_to_baton = {
      currentPilotElevation,
      currentPilotAirspeed,
      currentPilotHeading,
      currentPilotVerticalVelocity,
  };
}