#include "packet.h"
#include <ctime>
#include <chrono>

using namespace std;
using chrono::system_clock;
string generate_packet(vector<string> vec) {
    time_t system_time = system_clock::to_time_t(system_clock::now());
    string time = ctime(&system_time);
    string output =
        accumulate(vec.begin(), vec.end(), string(""), [](string a, string b) {
            return a + b + ";";
        });
    // return output + "\r\n";
    return time;
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