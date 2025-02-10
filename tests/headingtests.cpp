#include "plugin.h" // You'll need to create this header file
#include <gtest/gtest.h>

// Mock X-Plane's XPLMGetDataf function
float mock_XPLMGetDataf(XPLMDataRef inDataRef) {
  // Return some test values based on the DataRef
  if (inDataRef == heading)
    return 1000.0f / 3.28084f; // 1000 ft in meters
  if (inDataRef == elevationAglRef)
    return 500.0f / 3.28084f; // 500 ft in meters
  return 0.0f;
}

// Test fixture
class PluginTest : public ::testing::Test {
protected:
  void SetUp() override {
    // Set up mock function
    XPLMGetDataf = mock_XPLMGetDataf;
  }
};

// Sample test
TEST_F(PluginTest, TestElevationConversion) {
  float elevationMsl = XPLMGetDataf(elevationMslRef) * 3.28084f;
  EXPECT_NEAR(elevationMsl, 1000.0f, 0.1f);
}

// Add more tests here