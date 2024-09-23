#include <iostream>
#include <WinSock2.h>
#include <ws2tcpip.h>  // For IPv6-related structures
#define PORT 9999  // Valid port number

#pragma comment(lib, "Ws2_32.lib")

class TCPServer {
private:
    struct sockaddr_in6 srv;  // sockaddr_in6 for IPv6  
    int listeningSocket;
    int clientSocket;
    WSADATA ws;   ///client connect to test server 

public:
    TCPServer() : listeningSocket(-1), clientSocket(-1) {}

    ~TCPServer() {
        closeSockets();
        WSACleanup();  // Clean up Winsock
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

    bool getHostName() {
        char sHostName[32];
        int err = gethostname(sHostName, 32);
        if (err != 0) {
            std::cout << "Hostname unavailable" << std::endl;
            return false;
        }
        std::cout << "Hostname: " << sHostName << std::endl;
        return true;
    }

    bool createSocket() {
        listeningSocket = socket(AF_INET6, SOCK_STREAM, IPPROTO_TCP);
        if (listeningSocket < 0) {
            std::cout << "Failed to create socket." << std::endl;
            return false;
        }
        std::cout << "Socket created successfully." << std::endl;
        return true;
    }

    bool bindSocket() {
        srv.sin6_family = AF_INET6;
        srv.sin6_port = htons(PORT);
        srv.sin6_addr = in6addr_any;

        int err = bind(listeningSocket, (struct sockaddr*)&srv, sizeof(srv));
        if (err < 0) {
            std::cout << "Failed to bind socket to port." << std::endl;
            return false;
        }
        std::cout << "Socket bound to port successfully." << std::endl;
        return true;
    }

    bool listenForConnections(int backlog = 5) {
        int err = listen(listeningSocket, backlog);
        if (err < 0) {
            std::cout << "Backlog full, cannot listen." << std::endl;
            return false;
        }
        std::cout << "Listening for connections..." << std::endl;
        return true;
    }

    bool acceptConnection() {
        struct sockaddr_in client;
        int length = sizeof(client);

        clientSocket = accept(listeningSocket, (struct sockaddr*)&client, &length);
        if (clientSocket < 0) {
            std::cout << "Failed to accept connection." << std::endl;
            return false;
        }

        std::cout << "Connection accepted." << std::endl;
        std::cout << "Client IP Address: " << inet_ntoa(client.sin_addr) << std::endl;
        return true;
    }

    void closeSockets() {
        if (clientSocket >= 0) {
            closesocket(clientSocket);
            std::cout << "Client socket closed." << std::endl;
        }

        if (listeningSocket >= 0) {
            closesocket(listeningSocket);
            std::cout << "Listening socket closed." << std::endl;
        }
    }
};

int main() {
    TCPServer server;

    if (!server.initializeWinsock())
        return 1;

    if (!server.getHostName())
        return 1;

    if (!server.createSocket())
        return 1;

    if (!server.bindSocket())
        return 1;

    if (!server.listenForConnections())
        return 1;

    if (!server.acceptConnection())
        return 1;

    // Clean up resources before exiting
    return 0;
}