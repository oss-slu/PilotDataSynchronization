// #define TESTING
#include "../src/Logger.cpp"
#include <chrono>
#include <fstream>
#include <gtest/gtest.h>
#include <regex>
#include <thread>
Logger *Logger::instance = nullptr;

class LoggerTest : public ::testing::Test {
protected:
  void SetUp() override {
    // Clean up any existing log file before each test
    std::remove("xplane_plugin_log.txt");
  }

  void TearDown() override {
    Logger::resetInstance(); // Use the new resetInstance method
  }
};

// singleton
TEST(LoggerTest, SingletonPattern) {
  Logger *instance1 = Logger::getInstance();
  Logger *instance2 = Logger::getInstance();

  ASSERT_NE(instance1, nullptr);
  ASSERT_EQ(instance1, instance2);
}

// Test Last Message Update
TEST(LoggerTest, LastMessageUpdate) {
  Logger *logger = Logger::getInstance();
  std::string testMessage = "Test message";

  logger->log(testMessage);
  EXPECT_EQ(logger->get_last_message(), testMessage);
}

// Test Log Format
TEST(LoggerTest, LogFormat) {
  Logger *logger = Logger::getInstance();
  std::string testMessage = "Test log format";

  logger->log(testMessage);
  std::string logContent = logger->readLogFile();

  std::regex logPattern("\\[.*\\] Test log format");
  EXPECT_TRUE(std::regex_search(logContent, logPattern));
}

// Test Packet Count
TEST(LoggerTest, PacketCount) {
  Logger *logger = Logger::getInstance();
  logger->resetPacketCount();

  // Send multiple messages with SEND_PASS
  logger->log("Packet 1", MsgLogType::SEND_PASS);
  logger->log("Packet 2", MsgLogType::SEND_PASS);
  logger->log("Regular message", MsgLogType::NONE);
  logger->log("Packet 3", MsgLogType::SEND_PASS);

  EXPECT_EQ(logger->get_packets_sent(), 3);
}

// Test Last Status Update
TEST(LoggerTest, LastStatusUpdate) {
  Logger *logger = Logger::getInstance();

  logger->log("Test message", MsgLogType::SEND_PASS);
  EXPECT_EQ(logger->get_last_status(), MsgLogType::SEND_PASS);

  logger->log("Error message", MsgLogType::ERR);
  EXPECT_EQ(logger->get_last_status(), MsgLogType::ERR);
}

// test test (haha)
/* TEST(a, b) {
    std::cout << "Test is running" << std::endl;
    EXPECT_EQ(56, 56);
} */

int main(int argc, char **argv) {
  /*  std::cout << "Main function started" << std::endl;
   ::testing::InitGoogleTest(&argc, argv);
   return RUN_ALL_TESTS(); */

  std::cout << "Main function started" << std::endl;
  testing::InitGoogleTest(&argc, argv);
  std::cout << "Number of tests: "
            << testing::UnitTest::GetInstance()->total_test_count()
            << std::endl;
  testing::GTEST_FLAG(brief) = false;
  return RUN_ALL_TESTS();
}