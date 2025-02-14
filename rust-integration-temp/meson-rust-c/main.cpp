#include <iostream>

extern "C" void hello_from_rust();

int main() {
  std::cout << "Hello, world.\n";
  hello_from_rust();
  return 0;
}
