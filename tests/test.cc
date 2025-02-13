#include <gtest/gtest.h>
#include <iostream>

TEST(a, b) {
    std::cout << "Test is running" << std::endl;
    EXPECT_EQ(56, 56);
}

int main(int argc, char **argv) {
    std::cout << "Main function started" << std::endl;
    ::testing::InitGoogleTest(&argc, argv);
    return RUN_ALL_TESTS();
}