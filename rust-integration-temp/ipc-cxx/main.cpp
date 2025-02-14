#include <iostream>
#include "lib.rs.h"

int main() {
    auto thread_wrapper = new_wrapper();

    thread_wrapper->start();

    // must call stop to join the thread and not leave it dangling
    thread_wrapper->stop();

    return 0;
}
