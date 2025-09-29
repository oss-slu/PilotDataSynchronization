#include "threading-tools.h"
#include <gtest/gtest.h>
#include <regex>
#include <string>
#include <vector>

TEST(GeneratePacketTest, IncludesDataTimestampAndSource) {
  std::vector<std::string> data = {"123", "456", "789", "101"};
  std::string packet = generate_packet(data);

  // Check data values in order and separated by semicolons
  size_t pos = 0;
  for (const auto &val : data) {
    size_t found = packet.find(val, pos);
    ASSERT_NE(found, std::string::npos);
    pos = found + val.size();
  }

  ASSERT_EQ(std::count(packet.begin(), packet.end(), ';'), 5);

  ASSERT_NE(packet.find("X-Plane 11.55 PilotDataSync Plugin"),
            std::string::npos);

  ASSERT_TRUE(packet.size() >= 2 && packet.substr(packet.size() - 2) == "\r\n");

  // Extract timestamp
  size_t fourth = 0, fifth = 0, count = 0;
  for (size_t i = 0; i < packet.size(); ++i) {
    if (packet[i] == ';') {
      ++count;
      if (count == 4)
        fourth = i;
      if (count == 5) {
        fifth = i;
        break;
      }
    }
  }
  ASSERT_NE(fourth, 0u);
  ASSERT_NE(fifth, 0u);
  std::string timestamp = packet.substr(fourth + 1, fifth - fourth - 1);

  // This should check the timestamp format: YYYYMMDD HH:MM:SS
  std::regex ts_regex(R"(\d{8} \d{2}:\d{2}:\d{2})");
  ASSERT_TRUE(std::regex_match(timestamp, ts_regex));
  ASSERT_FALSE(timestamp.empty());
}

TEST(GeneratePacketTest, StructureAndValidation) {
  std::vector<std::string> data = {"A", "B", "C", "D"};
  std::string packet = generate_packet(data);

  // Should match: data1;data2;data3;data4;timestamp;source
  std::vector<std::string> parts;
  size_t start = 0, end;
  while ((end = packet.find(';', start)) != std::string::npos) {
    parts.push_back(packet.substr(start, end - start));
    start = end + 1;
  }
  // The last part (source + \r\n) is after the last semicolon
  size_t rn = packet.find("\r\n", start);
  ASSERT_NE(rn, std::string::npos);
  parts.push_back(packet.substr(start, rn - start));

  ASSERT_EQ(parts.size(), 6); // 4 data, timestamp, source

  // Timestamp should not be empty and should match format
  std::regex ts_regex(R"(\d{8} \d{2}:\d{2}:\d{2})");
  ASSERT_TRUE(std::regex_match(parts[4], ts_regex));
  ASSERT_FALSE(parts[4].empty());

  ASSERT_EQ(parts[5], "X-Plane 11.55 PilotDataSync Plugin");
}