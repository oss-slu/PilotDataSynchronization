#include <iostream>
#include "target/cxxbridge/baton/src/lib.rs.h"

int main() {
  std::cout << "[C++] BEGIN\n";
  auto thread_wrapper = new_wrapper();
  thread_wrapper->start();
  thread_wrapper->stop();
  std::cout << "[C++] DONE\n";

  return 0;
}
