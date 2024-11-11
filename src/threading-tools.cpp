#include "../include/threading-tools.h"
#include <ctime>
#include <chrono>
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
        accumulate(vec.begin(), vec.end(), string(""), [](string a, string b) {
            return a + b + ";";
        });
    string source = ";X-Plane 11.55 PilotDataSync Plugin;";
    return main_packet + buf + source + "\r\n";
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
