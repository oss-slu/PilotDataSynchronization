#include "../include/packet.h"

using namespace std;
#include <iostream>
#include <mutex>
#include <queue>
#include <string>
#include <thread>
#include <vector>
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
    int size() {
        lock_guard<mutex> lock(m);
        return q.size();
    }

    void push(ThreadMessage tm) {
        lock_guard<mutex> lock(m);
        q.push(tm);
    }

    ThreadMessage pop() {
        lock_guard<mutex> lock(m);
        ThreadMessage front = q.front();
        q.pop();
        return front;
    }
};

void t() {
    lock_guard<mutex> guard(mvec);
}

string generate_packet(vector<string> vec) {
    string output =
        accumulate(vec.begin(), vec.end(), string(""), [](string a, string b) {
            return a + b + ";";
        });
    return output + "\r\n";
}

string output_xml() {
    return "\
    <EventSource Version=\"1\" Id=\"QSensor\" Name=\"Affectiva Q Sensor\">\
        <Sample Id=\"AffectivaQSensor\" Name=\"QSensor\">\
            <Field Id=\"SeqNo\" />\
            <Field Id=\"AccelZ\" />\
            <Field Id=\"AccelY\" />\
            <Field Id=\"AccelX\" />\
            <Field Id=\"Battery\" />\
            <Field Id=\"Temperature\" Range=\"Fixed\" Min=\"30\" Max=\"40\" />\
            <Field Id=\"EDA\" Range=\"Variable\" Min=\"0\" Max=\"0.2\" />\
        </Sample>\
    </EventSource>";
}