// #define TESTING
#include <chrono>
#include <fstream>
#include <gtest/gtest.h>
#include <regex>
#include <thread>

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
