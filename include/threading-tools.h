#include <iterator>
#include <numeric>
#include <string>
#include <vector>
#include <iostream>
#include <mutex>
#include <queue>
#include <thread>
using namespace std;

struct ThreadMessage {
    const float* values_for_packet;
    const bool end_execution_flag;

    ThreadMessage(float const (&values_for_packet)[4], const bool tf) :
        values_for_packet(values_for_packet),
        end_execution_flag(tf) {};
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