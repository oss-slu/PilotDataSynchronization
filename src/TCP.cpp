#include <WinSock2.h>
#include <ws2tcpip.h> // For IPv6-related structures

#include <iostream>
#define PORT 5000 // The server's port number

#pragma comment(lib, "Ws2_32.lib")

class TCPClient {
  private:
    struct sockaddr_in6 server; // sockaddr_in6 for IPv6
    int clientSocket;
    WSADATA ws;

  public:
    TCPClient() : clientSocket(-1) {}

    ~TCPClient() {
        closeSocket();
        WSACleanup(); // Clean up Winsock
    }

    bool initializeWinsock() {
        int err = WSAStartup(MAKEWORD(2, 2), &ws);
        if (err != 0) {
            std::cout << "Failed to initialize Winsock" << std::endl;
            return false;
        }
        std::cout << "Winsock initialized successfully!" << std::endl;
        return true;
    }

    bool createSocket() {
        clientSocket = socket(AF_INET6, SOCK_STREAM, IPPROTO_TCP);
        if (clientSocket < 0) {
            std::cout << "Failed to create socket." << std::endl;
            return false;
        }

        std::cout << "Socket created successfully." << std::endl;
        return true;
    }

    bool connectToServer(const std::string& serverIP) {
        // Define the server address
        server.sin6_family = AF_INET6;
        server.sin6_port = htons(PORT);

        // Convert server IP from text to binary form
        if (inet_pton(AF_INET6, serverIP.c_str(), &server.sin6_addr) <= 0) {
            std::cout << "Invalid server IP address." << std::endl;
            return false;
        }

        // Connect to the server
        int err =
            connect(clientSocket, (struct sockaddr*)&server, sizeof(server));
        if (err < 0) {
            std::cout << "Failed to connect to server." << std::endl;
            return false;
        }

        std::cout << "Connected to server: " << serverIP << std::endl;
        return true;
    }

    void closeSocket() {
        if (clientSocket >= 0) {
            closesocket(clientSocket);
            std::cout << "Client socket closed." << std::endl;
        }
    }
};

int main() {
    TCPClient client;

    if (!client.initializeWinsock())
        return 1;

    if (!client.createSocket())
        return 1;

    std::string serverIP = "::1"; // IPv6 loopback address

    if (!client.connectToServer(serverIP))
        return 1;

    return 0;
}