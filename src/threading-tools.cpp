#include "../include/threading-tools.h"
#include <iostream>
#include <mutex>
#include <queue>
#include <string>
#include <thread>
#include <vector>
using namespace std;

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