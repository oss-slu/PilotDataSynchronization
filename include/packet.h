#include <iterator>
#include <numeric>
#include <string>
#include <vector>

#include <iostream>
#include <mutex>
#include <queue>
#include <string>
#include <thread>
#include <vector>
using namespace std;

vector<int> vec;
mutex mvec;

struct ThreadMessage;

class ThreadQueue;

string generate_packet(vector<string> vec);

string output_xml();