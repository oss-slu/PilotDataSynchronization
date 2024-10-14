#include <gtest/gtest.h>
#include "plugin.h"  // You'll need to create this header file

// Mock X-Plane's XPLMGetDataf function
float mock_XPLMGetDataf(XPLMDataRef inDataRef) {
    // Return some test values based on the DataRef
    if (inDataRef == headingPilotRef) return 1000.0f / 3.28084f;  // 1000 ft in meters
    if (inDataRef == headingCopilotRef) return 500.0f / 3.28084f; 
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
    float headingPilotNum = XPLMGetDataf(elevationMslRef) * 3.28084f;
    float headingCopilotNum = XPLMGetDataf(elevationMslRef) * 3.28084f;

    EXPECT_NEAR(headingPilotNum, 1000.0f, 0.1f) && EXPECT_NEAR(headingCopilotNum, 1000.0f, 0.1f);
}

// Add more tests here