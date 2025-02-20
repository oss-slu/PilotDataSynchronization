#pragma once
#include <iostream>
#include <iterator>
#include <mutex>
#include <numeric>
#include <queue>
#include <string>
#include <thread>
#include <vector>
using namespace std;

struct ThreadMessage {
  const float values_for_packet[4];
  const bool end_execution_flag;

  ThreadMessage(float const (&values)[4], const bool tf)
      : values_for_packet{values[0], values[1], values[2], values[3]},
        end_execution_flag(tf){};
};

class ThreadQueue {
private:
  mutex m;
  queue<ThreadMessage> q;

public:
  int size();

  void push(ThreadMessage tm);

  ThreadMessage pop();
};

string generate_packet(vector<string> vec);

string output_xml();
