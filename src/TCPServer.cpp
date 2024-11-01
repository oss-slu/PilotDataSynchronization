#include <WinSock2.h>
#include <ws2tcpip.h>
#include <iostream>
#include <string>

#define PORT 8089
#define BUFFER_SIZE 1024
#pragma comment(lib, "Ws2_32.lib")

class TCPServer {
private:
    WSADATA ws;
    int serverSocket;
    int clientSocket;
    struct sockaddr_in server;  // Changed to IPv4
    struct sockaddr_in client;  // Changed to IPv4

public:
    TCPServer() : serverSocket(-1), clientSocket(-1) {
        std::cout << "Server starting up..." << std::endl;
    }
    
    ~TCPServer() {
        closeSocket();
        WSACleanup();
        std::cout << "Server shut down." << std::endl;
    }

    bool initializeWinsock() {
        std::cout << "Initializing Winsock..." << std::endl;
        int err = WSAStartup(MAKEWORD(2, 2), &ws);
        if (err != 0) {
            std::cout << "Failed to initialize Winsock. Error code: " << err << std::endl;
            return false;
        }
        std::cout << "Winsock initialized successfully!" << std::endl;
        return true;
    }

    bool createSocket() {
        std::cout << "Creating socket..." << std::endl;
        serverSocket = socket(AF_INET, SOCK_STREAM, IPPROTO_TCP);  // Changed to IPv4
        if (serverSocket < 0) {
            std::cout << "Failed to create socket. Error code: " << WSAGetLastError() << std::endl;
            return false;
        }

        // Allow socket reuse
        int opt = 1;
        if (setsockopt(serverSocket, SOL_SOCKET, SO_REUSEADDR, (char*)&opt, sizeof(opt)) < 0) {
            std::cout << "Failed to set socket option. Error code: " << WSAGetLastError() << std::endl;
            return false;
        }

        std::cout << "Socket created successfully." << std::endl;
        return true;
    }

    bool bindSocket() {
        std::cout << "Binding socket to port " << PORT << "..." << std::endl;
        server.sin_family = AF_INET;  // Changed to IPv4
        server.sin_addr.s_addr = INADDR_ANY;  // Changed to IPv4
        server.sin_port = htons(PORT);

        if (bind(serverSocket, (struct sockaddr*)&server, sizeof(server)) < 0) {
            std::cout << "Failed to bind socket. Error code: " << WSAGetLastError() << std::endl;
            return false;
        }
        std::cout << "Socket bound successfully." << std::endl;
        return true;
    }

    bool listenForConnections() {
        std::cout << "Starting to listen for connections..." << std::endl;
        if (listen(serverSocket, 1) < 0) {
            std::cout << "Failed to listen on socket. Error code: " << WSAGetLastError() << std::endl;
            return false;
        }
        std::cout << "Server listening on port " << PORT << std::endl;
        return true;
    }

    bool acceptConnection() {
        std::cout << "Waiting for client connection..." << std::endl;
        int clientLen = sizeof(client);
        clientSocket = accept(serverSocket, (struct sockaddr*)&client, &clientLen);
        if (clientSocket < 0) {
            std::cout << "Failed to accept connection. Error code: " << WSAGetLastError() << std::endl;
            return false;
        }
        
        char clientIP[INET_ADDRSTRLEN];  // Changed to IPv4
        inet_ntop(AF_INET, &(client.sin_addr), clientIP, INET_ADDRSTRLEN);  // Changed to IPv4
        std::cout << "Client connected from IP: " << clientIP << std::endl;
        return true;
    }

    void receiveData() {
        std::cout << "Ready to receive data..." << std::endl;
        char buffer[BUFFER_SIZE];
        while (true) {
            std::cout << "Waiting for data..." << std::endl;
            int bytesReceived = recv(clientSocket, buffer, BUFFER_SIZE - 1, 0);
            if (bytesReceived < 0) {
                std::cout << "Error receiving data. Error code: " << WSAGetLastError() << std::endl;
                break;
            }
            if (bytesReceived == 0) {
                std::cout << "Client disconnected." << std::endl;
                break;
            }
            buffer[bytesReceived] = '\0';
            std::cout << "Received " << bytesReceived << " bytes: " << buffer << std::endl;
        }
    }

    void closeSocket() {
        if (clientSocket >= 0) {
            closesocket(clientSocket);
            std::cout << "Client socket closed." << std::endl;
        }
        if (serverSocket >= 0) {
            closesocket(serverSocket);
            std::cout << "Server socket closed." << std::endl;
        }
    }
};

int main() {
    std::cout << "TCP Server Starting..." << std::endl;
    TCPServer server;
    
    if (!server.initializeWinsock()) {
        std::cout << "Failed to initialize Winsock" << std::endl;
        return 1;
    }
    
    if (!server.createSocket()) {
        std::cout << "Failed to create socket" << std::endl;
        return 1;
    }
    
    if (!server.bindSocket()) {
        std::cout << "Failed to bind socket" << std::endl;
        return 1;
    }
    
    if (!server.listenForConnections()) {
        std::cout << "Failed to listen for connections" << std::endl;
        return 1;
    }
    
    if (!server.acceptConnection()) {
        std::cout << "Failed to accept connection" << std::endl;
        return 1;
    }
    
    server.receiveData();
    
    std::cout << "Server shutting down..." << std::endl;
    return 0;
}