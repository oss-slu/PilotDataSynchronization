#include "threading-tools.h"
#include <chrono>
#include <ctime>
using namespace std;
using chrono::system_clock;

int ThreadQueue::size() {
  lock_guard<mutex> lock(m);
  return q.size();
}

void ThreadQueue::push(ThreadMessage tm) {
  lock_guard<mutex> lock(m);
  q.push(tm);
}

ThreadMessage ThreadQueue::pop() {
  lock_guard<mutex> lock(m);
  if (this->q.size() < 1) {
    throw std::invalid_argument("Attempted to pop off an empty queue");
  }
  ThreadMessage front = q.front();
  q.pop();
  return front;
}

string generate_packet(vector<string> vec) {
  time_t system_time = system_clock::to_time_t(system_clock::now());
  auto gm = gmtime(&system_time);
  char buf[42];
  strftime(buf, 42, "%Y%m%d %X", gm);
  string main_packet =
      accumulate(vec.begin(), vec.end(), string(""),
                 [](string a, string b) { return a + b + ";"; });
  string source = ";X-Plane 11.55 PilotDataSync Plugin;";
  return main_packet + buf + source + "\r\n";
}
