#include "threading-tools.h"
#include <gtest/gtest.h>
#include <regex>
#include <string>
#include <vector>
#include <chrono>
#include <iomanip>
#include <sstream>

//Changed to accept 4 values
std::string generate_packet(const std::string& p1, const std::string& p2, const std::string& p3, const std::string& p4) {
    auto now = std::chrono::system_clock::now();
    std::time_t t = std::chrono::system_clock::to_time_t(now);
    std::tm tm;
#ifdef _WIN32
    localtime_s(&tm, &t);
#else
    localtime_r(&t, &tm);
#endif
    std::ostringstream oss;
    oss << p1 << ";" << p2 << ";" << p3 << ";" << p4 << ";";
    oss << std::put_time(&tm, "%Y%m%d %H:%M:%S") << ";";
    oss << "X-Plane 11.55 PilotDataSync Plugin\r\n";
    return oss.str();
}

TEST(GeneratePacketTest, IncludesDataTimestampAndSource) {
    std::string packet = generate_packet("123", "456", "789", "101");

    // Check data values in order and separated by semicolons
    ASSERT_NE(packet.find("123;456;789;101;"), std::string::npos);
    ASSERT_EQ(std::count(packet.begin(), packet.end(), ';'), 5);
    ASSERT_NE(packet.find("X-Plane 11.55 PilotDataSync Plugin"), std::string::npos);
    ASSERT_TRUE(packet.size() >= 2 && packet.substr(packet.size() - 2) == "\r\n");

    // Extract timestamp
    size_t fourth = 0, fifth = 0, count = 0;
    for (size_t i = 0; i < packet.size(); ++i) {
        if (packet[i] == ';') {
            ++count;
            if (count == 4) fourth = i;
            if (count == 5) { fifth = i; break; }
        }
    }
    ASSERT_NE(fourth, 0u);
    ASSERT_NE(fifth, 0u);
    std::string timestamp = packet.substr(fourth + 1, fifth - fourth - 1);

    std::regex ts_regex(R"(\d{8} \d{2}:\d{2}:\d{2})");
    ASSERT_TRUE(std::regex_match(timestamp, ts_regex));
    ASSERT_FALSE(timestamp.empty());
}

TEST(GeneratePacketTest, StructureAndValidation) {
    std::string packet = generate_packet("A", "B", "C", "D");

    std::vector<std::string> parts;
    size_t start = 0, end;
    while ((end = packet.find(';', start)) != std::string::npos) {
        parts.push_back(packet.substr(start, end - start));
        start = end + 1;
    }
    size_t rn = packet.find("\r\n", start);
    ASSERT_NE(rn, std::string::npos);
    parts.push_back(packet.substr(start, rn - start));

    ASSERT_EQ(parts.size(), 6); // 4 data, timestamp, source

    std::regex ts_regex(R"(\d{8} \d{2}:\d{2}:\d{2})");
    ASSERT_TRUE(std::regex_match(parts[4], ts_regex));
    ASSERT_FALSE(parts[4].empty());

    ASSERT_EQ(parts[5], "X-Plane 11.55 PilotDataSync Plugin");
}